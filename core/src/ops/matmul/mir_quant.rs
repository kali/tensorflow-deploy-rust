use ops::binary::wire_with_rank_broadcast;

use crate::internal::*;
use crate::ops;
use crate::ops::matmul::*;

use super::mir_quant_unary::QMatMulUnary;

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct MatMulQParams {
    pub a0: AttrOrInput,
    pub a_scale: AttrOrInput,
    pub b0: AttrOrInput,
    pub b_scale: AttrOrInput,
    pub c0: AttrOrInput,
    pub c_scale: AttrOrInput,
}

impl MatMulQParams {
    pub fn noop_static(dt: DatumType) -> MatMulQParams {
        MatMulQParams {
            a0: AttrOrInput::Attr(Tensor::zero_scalar_dt(dt).unwrap().into_arc_tensor()),
            a_scale: AttrOrInput::Attr(rctensor0(1f32)),
            b0: AttrOrInput::Attr(Tensor::zero_scalar_dt(dt).unwrap().into_arc_tensor()),
            b_scale: AttrOrInput::Attr(rctensor0(1f32)),
            c0: AttrOrInput::Attr(Tensor::zero_scalar_dt(dt).unwrap().into_arc_tensor()),
            c_scale: AttrOrInput::Attr(rctensor0(1f32)),
        }
    }

    pub fn all_dynamic(offset: usize) -> MatMulQParams {
        MatMulQParams {
            a0: AttrOrInput::Input(offset),
            a_scale: AttrOrInput::Input(offset + 1),
            b0: AttrOrInput::Input(offset + 2),
            b_scale: AttrOrInput::Input(offset + 3),
            c0: AttrOrInput::Input(offset + 4),
            c_scale: AttrOrInput::Input(offset + 5),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &AttrOrInput)> {
        vec![
            ("a0", &self.a0),
            ("a_scale", &self.a_scale),
            ("b0", &self.b0),
            ("b_scale", &self.b_scale),
            ("c0", &self.c0),
            ("c_scale", &self.c_scale),
        ]
        .into_iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut AttrOrInput)> {
        vec![
            ("a0", &mut self.a0),
            ("a_scale", &mut self.a_scale),
            ("b0", &mut self.b0),
            ("b_scale", &mut self.b_scale),
            ("c0", &mut self.c0),
            ("c_scale", &mut self.c_scale),
        ]
        .into_iter()
    }

    pub fn inline_static(
        &self,
        model: &TypedModel,
        node: &TypedNode,
    ) -> TractResult<Option<(Vec<OutletId>, MatMulQParams)>> {
        let mut new = self.clone();
        let mut inputs = vec![];
        for (ix, input) in node.inputs.iter().enumerate() {
            if let (Some(position), Some(k)) = (
                self.iter().position(|qp| &AttrOrInput::Input(ix) == qp.1),
                model.outlet_fact(*input)?.konst.as_ref(),
            ) {
                *new.iter_mut().nth(position).unwrap().1 = AttrOrInput::Attr(k.clone());
                for qp in new.iter_mut() {
                    qp.1.remove_input(position);
                }
            } else {
                inputs.push(*input)
            }
        }
        Ok(Some((inputs, new)).filter(|pair| &pair.1 != self))
    }

    pub fn remove_input(&mut self, ix: usize) {
        for qp in self.iter_mut() {
            if let AttrOrInput::Input(slot) = qp.1 {
                *slot = *slot - (*slot > ix) as usize;
            }
        }
    }

    pub fn insert_input(&mut self, ix: usize) {
        for qp in self.iter_mut() {
            if let AttrOrInput::Input(slot) = qp.1 {
                *slot = *slot + (*slot >= ix) as usize;
            }
        }
    }

    pub fn input_count(&self) -> usize {
        self.iter().filter(|qp| matches!(qp.1, AttrOrInput::Input(_))).count()
    }
}

#[derive(Debug, Clone, new, Hash)]
pub struct QMatMul {
    pub a_trans: bool,
    pub b_trans: bool,
    pub c_trans: bool,
    pub output_type: DatumType,
    pub params: MatMulQParams,
}

impl_dyn_hash!(QMatMul);

impl QMatMul {
    pub fn with_a_trans(self, a_trans: bool) -> QMatMul {
        QMatMul { a_trans, ..self }
    }

    pub fn with_b_trans(self, b_trans: bool) -> QMatMul {
        QMatMul { b_trans, ..self }
    }

    pub fn with_c_trans(self, c_trans: bool) -> QMatMul {
        QMatMul { c_trans, ..self }
    }
}

impl Op for QMatMul {
    fn name(&self) -> Cow<str> {
        "QMatMul".into()
    }

    op_core_mir!();
    op_as_typed_op!();
}

impl EvalOp for QMatMul {
    fn is_stateless(&self) -> bool {
        true
    }

    fn eval(&self, inputs: TVec<Arc<Tensor>>) -> TractResult<TVec<Arc<Tensor>>> {
        if &inputs[0].rank() != &inputs[1].rank() {
            bail!("Rank mismatch {:?} vs {:?}", inputs[0], inputs[1]);
        }

        let mut model = TypedModel::default();
        let a = model.add_const("source_a", inputs[0].clone())?;
        let b = model.add_const("source_b", inputs[1].clone())?;
        let bias = model.add_const("source_bias", inputs[2].clone())?;

        let params = self
            .params
            .iter()
            .map(|(name, qp)| {
                model.add_const(format!("source_{}", name), qp.tensor(&inputs).clone())
            })
            .collect::<TractResult<Vec<_>>>()?;

        let new_op = MatMul { a_trans: self.a_trans, b_trans: self.b_trans, c_trans: self.c_trans };
        let result = model.wire_node("adhoc.matmul", new_op, &[a, b])?[0];
        let result = wire_matmul_quant(
            &mut model,
            "adhoc",
            a,
            self.a_trans,
            b,
            self.b_trans,
            Some(bias),
            self.c_trans,
            result,
            self.output_type,
            &params,
        )?;
        model.set_output_outlets(&[result])?;
        model.into_runnable()?.run(tvec![])
    }
}

impl TypedOp for QMatMul {
    fn output_facts(&self, inputs: &[&TypedFact]) -> TractResult<TVec<TypedFact>> {
        if inputs.len() != 3 + self.params.input_count() {
            bail!(
                "Inconsistent q matmul. expects {} inputs, got {}",
                3 + self.params.input_count(),
                inputs.len()
            );
        }
        if inputs[0].rank() != inputs[1].rank() {
            bail!(
                "Inconsistent matmul between {:?} and {:?} (rank mismatch)",
                inputs[0],
                inputs[1]
            );
        }
        let (_m, _k, _n, c_shape) = compute_shape(
            &inputs[0].shape,
            &inputs[1].shape,
            self.a_trans,
            self.b_trans,
            self.c_trans,
        )?;
        Ok(tvec!(TypedFact::dt_shape(self.output_type, c_shape)))
    }

    fn declutter(
        &self,
        model: &TypedModel,
        node: &TypedNode,
    ) -> TractResult<Option<TypedModelPatch>> {
        let a_fact = model.outlet_fact(node.inputs[0])?;
        let b_fact = model.outlet_fact(node.inputs[1])?;
        let bias_fact = model.outlet_fact(node.inputs[2])?;

        if bias_fact.konst.is_none() {
            return Ok(None);
        }

        let konst_ix = if a_fact.konst.is_some() {
            0
        } else if b_fact.konst.is_some() {
            1
        } else {
            return Ok(None);
        };

        let flip = konst_ix == 1;
        let t_konst = [self.a_trans, self.b_trans][konst_ix] ^ flip;
        let t_var = [self.b_trans, self.a_trans][konst_ix] ^ flip;
        let konst = model.outlet_fact(node.inputs[konst_ix])?.konst.clone().unwrap();
        let bias = model.outlet_fact(node.inputs[2])?.konst.clone().unwrap();

        let inputs: Vec<_> = node
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(i, out_id)| if i == konst_ix || i == 2 { None } else { Some(*out_id) })
            .collect();

        let new_params = {
            let mut qp = self.params.clone();
            //compensate for the removed parameter
            for (_, a) in qp.iter_mut() {
                if let AttrOrInput::Input(i) = a {
                    *i -= 2
                }
            }
            if flip {
                MatMulQParams {
                    a0: qp.b0,
                    a_scale: qp.b_scale,
                    b0: qp.a0,
                    b_scale: qp.a_scale,
                    c0: qp.c0,
                    c_scale: qp.c_scale,
                }
            } else {
                qp
            }
        };

        TypedModelPatch::replace_single_op(
            model,
            node,
            &inputs,
            QMatMulUnary::new(
                konst,
                // if bias is uniformly zero, it can be discarded
                Some(bias).filter(|b| {
                    b.as_uniform()
                        .map(|b| b.cast_to_scalar::<f32>().unwrap() != 0.0)
                        .unwrap_or(false)
                }),
                t_konst,
                t_var,
                self.c_trans ^ flip,
                self.output_type,
                new_params,
            ),
        )
        .map(Some)
    }

    fn cost(&self, inputs: &[&TypedFact]) -> TractResult<TVec<(Cost, TDim)>> {
        cost(
            &inputs[0].shape.to_tvec(),
            &inputs[1].shape.to_tvec(),
            inputs[0].datum_type,
            self.a_trans,
            self.b_trans,
        )
    }

    fn codegen(
        &self,
        model: &TypedModel,
        node: &TypedNode,
    ) -> TractResult<Option<TypedModelPatch>> {
        let mut patch = TypedModelPatch::default();

        if let Some((inputs, qp)) = self.params.inline_static(model, node)? {
            let mut patch = TypedModelPatch::new("inlining matmul quantized params");
            let inputs: Vec<OutletId> =
                inputs.iter().map(|i| patch.tap_model(model, *i)).collect::<TractResult<_>>()?;
            let op = Self { params: qp, ..self.clone() };
            let wire = patch.wire_node(&node.name, op, &inputs)?;
            patch.shunt_outside(model, node.id.into(), wire[0])?;
            return Ok(Some(patch));
        }

        let a = patch.tap_model(model, node.inputs[0])?;
        let b = patch.tap_model(model, node.inputs[1])?;
        let bias = patch.tap_model(model, node.inputs[2])?;

        let params = self
            .params
            .iter()
            .map(|(name, qp)| match qp {
                AttrOrInput::Input(o) => patch.tap_model(model, node.inputs[*o]),
                AttrOrInput::Attr(t) => {
                    patch.add_const(format!("{}_{}", node.name, name), t.clone())
                }
            })
            .collect::<TractResult<Vec<OutletId>>>()?;

        let new_op = MatMul { a_trans: self.a_trans, b_trans: self.b_trans, c_trans: self.c_trans };
        let result = patch.wire_node(format!("{}.matmul", &node.name), new_op, &[a, b])?[0];
        let result = wire_matmul_quant(
            &mut patch,
            &node.name,
            a,
            self.a_trans,
            b,
            self.b_trans,
            Some(bias),
            self.c_trans,
            result,
            self.output_type,
            &params,
        )?;
        patch.shunt_outside(model, node.id.into(), result)?;
        Ok(Some(patch))
    }

    as_op!();
}

pub(crate) fn wire_matmul_quant(
    model: &mut TypedModel,
    name: &str,
    a: OutletId,
    a_trans: bool,
    b: OutletId,
    b_trans: bool,
    bias: Option<OutletId>,
    c_trans: bool,
    mut result: OutletId,
    output_type: DatumType,
    params: &[OutletId],
) -> TractResult<OutletId> {
    let a_fact = model.outlet_fact(a)?.clone();
    let rank = a_fact.rank();
    let m_axis = rank - 2 + c_trans as usize;
    let n_axis = rank - 1 - c_trans as usize;
    let result_fact = model.outlet_fact(result)?.clone();

    if let Some(bias) = bias {
        let bias_fact = model.outlet_fact(bias)?.clone();
        if bias_fact.rank() == 2 {
            let expected_bias_shape: [TDim; 2] = if c_trans {
                [1.to_dim(), result_fact.shape[rank - 1].clone()]
            } else {
                [result_fact.shape[rank - 2].clone(), 1.to_dim()]
            };
            assert_eq!(&**bias_fact.shape, expected_bias_shape);
        } else {
            assert_eq!(bias_fact.shape.iter().product::<TDim>(), 1.to_dim());
        };

        result = wire_with_rank_broadcast(
            &format!("{}.add_bias", &name),
            model,
            ops::math::add::bin_typed(),
            &[result, bias],
        )?[0];
    }

    let k = model.outlet_fact(a)?.shape[rank - 2 + !a_trans as usize].clone();

    let abc_scale = combine_scales(model, name, params[1], params[3], params[5])?;

    let a_i32 =
        model.wire_node(format!("{}.a_as_i32", name), ops::cast::cast(i32::datum_type()), &[a])?[0];
    let b_i32 =
        model.wire_node(format!("{}.b_as_i32", name), ops::cast::cast(i32::datum_type()), &[b])?[0];
    let a_k_axis = rank - 2 + !a_trans as usize;
    let sum_a = model.wire_node(
        format!("{}.sum_a", name),
        ops::nn::Reduce::new(tvec!(a_k_axis), ops::nn::Reducer::Sum),
        &[a_i32],
    )?[0];
    let sum_a =
        model.wire_node(format!("{}.sum_a_reduced", name), AxisOp::Rm(a_k_axis), &[sum_a])?[0];
    let b_k_axis = rank - 2 + b_trans as usize;
    let sum_b = model.wire_node(
        format!("{}.sum_b", name),
        ops::nn::Reduce::new(tvec!(b_k_axis), ops::nn::Reducer::Sum),
        &[b_i32],
    )?[0];
    let sum_b =
        model.wire_node(format!("{}.sum_b_reduced", name), AxisOp::Rm(b_k_axis), &[sum_b])?[0];
    let result = compensate_zero_points(
        model, name, result, k, params[0], params[2], sum_a, sum_b, m_axis, n_axis,
    )?;
    requant(model, name, result, output_type, abc_scale, params[4])
}

pub(crate) fn combine_scales(
    model: &mut TypedModel,
    name: &str,
    a_scale: OutletId,
    b_scale: OutletId,
    c_scale: OutletId,
) -> TractResult<OutletId> {
    let ab_scale = wire_with_rank_broadcast(
        &format!("{}.ab_scale", name),
        model,
        ops::math::mul::bin_typed(),
        &[a_scale, b_scale],
    )?[0];
    let abc_scale = wire_with_rank_broadcast(
        &format!("{}.abc_scales", name),
        model,
        ops::math::div::bin_typed(),
        &[ab_scale, c_scale],
    )?[0];
    Ok(abc_scale)
}

pub(crate) fn compensate_zero_points(
    model: &mut TypedModel,
    name: &str,
    result: OutletId,
    k: TDim,
    a0: OutletId,
    b0: OutletId,
    sum_a: OutletId,
    sum_b: OutletId,
    m_axis: usize,
    n_axis: usize,
) -> TractResult<OutletId> {
    let input_shape = model.outlet_fact(result)?.shape.clone();
    let rank = model.outlet_fact(result)?.rank();

    debug_assert_eq!(model.outlet_fact(sum_a)?.rank(), rank - 1);
    debug_assert_eq!(model.outlet_fact(sum_b)?.rank(), rank - 1);

    // make sum_a into from a 1D vector to a vertical matrix, sum_b horizontal
    // switch shapes if c_trans
    let sum_a =
        model.wire_node(format!("{}.reshape_sum_a", name), AxisOp::Add(n_axis), &[sum_a])?[0];

    let sum_b =
        model.wire_node(format!("{}.reshape_sum_b", name), AxisOp::Add(m_axis), &[sum_b])?[0];

    debug_assert_eq!(
        model.outlet_fact(sum_a)?.shape[m_axis],
        model.outlet_fact(result)?.shape[m_axis]
    );
    debug_assert_eq!(
        model.outlet_fact(sum_b)?.shape[n_axis],
        model.outlet_fact(result)?.shape[n_axis]
    );

    let a0 =
        model.wire_node(format!("{}.cast_a0", name), ops::cast::cast(i32::datum_type()), &[a0])?[0];

    let b0 =
        model.wire_node(format!("{}.cast_b0", name), ops::cast::cast(i32::datum_type()), &[b0])?[0];

    let k = model.add_const(format!("{}.k", name), rctensor0(k.clone()))?;
    let k =
        model.wire_node(format!("{}.cast_k", name), ops::cast::cast(i32::datum_type()), &[k])?[0];

    let a0_sum_b = wire_with_rank_broadcast(
        &format!("{}.a0_sum_b", name),
        model,
        ops::math::mul::bin_typed(),
        &[a0, sum_b],
    )?[0];

    let b0_sum_a = wire_with_rank_broadcast(
        &format!("{}.b0_sum_a", name),
        model,
        ops::math::mul::bin_typed(),
        &[b0, sum_a],
    )?[0];

    let a0_k = wire_with_rank_broadcast(
        &format!("{}.a0_k", name),
        model,
        ops::math::mul::bin_typed(),
        &[a0, k],
    )?[0];

    let a0_k_b0 = wire_with_rank_broadcast(
        &format!("{}.a0_k_b0", name),
        model,
        ops::math::mul::bin_typed(),
        &[a0_k, b0],
    )?[0];

    let result = wire_with_rank_broadcast(
        &format!("{}.minus_a0_B", &name),
        model,
        ops::math::sub::bin_typed(),
        &[result, a0_sum_b],
    )?[0];
    let result = wire_with_rank_broadcast(
        &format!("{}.minus_b0_A", &name),
        model,
        ops::math::sub::bin_typed(),
        &[result, b0_sum_a],
    )?[0];

    let result = wire_with_rank_broadcast(
        &format!("{}.plus_a0_k_b0", &name),
        model,
        ops::math::add::bin_typed(),
        &[result, a0_k_b0],
    )?[0];

    debug_assert_eq!(model.outlet_fact(result)?.shape, input_shape);
    Ok(result)
}

pub(crate) fn requant(
    model: &mut TypedModel,
    name: &str,
    wire: OutletId,
    dt: DatumType,
    scale: OutletId,
    zero_point: OutletId,
) -> TractResult<OutletId> {
    let wire = wire_with_rank_broadcast(
        &format!("{}.scale", name),
        model,
        ops::quant::scale::bin_typed(),
        &[scale, wire],
    )?[0];

    let zero_point = model.wire_node(
        format!("{}.cast_c0", name),
        ops::cast::cast(i32::datum_type()),
        &[zero_point],
    )?[0];

    let wire = wire_with_rank_broadcast(
        &format!("{}.zeropoint", name),
        model,
        ops::math::add::bin_typed(),
        &[wire, zero_point],
    )?[0];

    clamp_and_cast_to(model, name, dt, wire)
}

pub(crate) fn clamp_and_cast_to(
    model: &mut TypedModel,
    name: &str,
    dt: DatumType,
    wire: OutletId,
) -> TractResult<OutletId> {
    if dt == i32::datum_type() {
        return Ok(wire);
    }
    let rank = model.outlet_fact(wire)?.rank();
    let inf = dt
        .min_value()
        .cast_to_dt(DatumType::I32)?
        .into_owned()
        .broadcast_into_rank(rank)?
        .into_arc_tensor();
    let sup = dt
        .max_value()
        .cast_to_dt(DatumType::I32)?
        .into_owned()
        .broadcast_into_rank(rank)?
        .into_arc_tensor();
    let wire = model.wire_node(format!("{}.min", name), ops::math::min::unary(sup), &[wire])?;
    let wire = model.wire_node(format!("{}.max", name), ops::math::max::unary(inf), &wire)?;
    let wire = model.wire_node(format!("{}.cast", name), ops::cast::cast(dt), &wire)?;
    Ok(wire[0])
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::collection::vec;
    use proptest::prelude::*;
    use tract_ndarray::prelude::*;

    proptest! {
        #[test]
        fn prop(pb in any::<QMatMulProblem>()) {
            pb.check()
        }
    }

    #[test]
    fn c0() {
        QMatMulProblem {
            a: arr2(&[[0]]),
            b: arr2(&[[0]]),
            bias: arr2(&[[0]]),
            a0: 0,
            b0: 0,
            c0: 1,
            a_scale: 1.0,
            b_scale: 1.0,
            c_scale: 1.0,
        }
        .check()
    }

    #[test]
    fn b_scale() {
        QMatMulProblem {
            a: arr2(&[[0]]),
            b: arr2(&[[0]]),
            bias: arr2(&[[0]]),
            a0: 0,
            b0: 0,
            c0: 1,
            a_scale: 1.0,
            b_scale: 2.0,
            c_scale: 1.0,
        }
        .check();
    }

    #[test]
    fn sat() {
        QMatMulProblem {
            a: arr2(&[[0]]),
            b: arr2(&[[34]]),
            bias: arr2(&[[0]]),
            a0: -17,
            b0: 1,
            c0: 0,
            a_scale: 1.0,
            b_scale: 0.05,
            c_scale: 0.25,
        }
        .check();
    }

    #[test]
    fn rounding() {
        QMatMulProblem {
            a: arr2(&[[26]]),
            b: arr2(&[[0]]),
            bias: arr2(&[[0]]),
            a0: 27,
            b0: -1,
            c0: 1,
            a_scale: 1.0,
            b_scale: 0.05,
            c_scale: 1.0,
        }
        .check();
    }

    #[test]
    fn neg_rounding() {
        QMatMulProblem {
            a: arr2(&[[-23]]),
            b: arr2(&[[-2]]),
            bias: arr2(&[[0]]),
            a0: -11,
            b0: -45,
            c0: 0,
            a_scale: 0.1,
            b_scale: 1.0,
            c_scale: 1.0,
        }
        .check();
    }

    #[test]
    fn rounding_ties_2() {
        QMatMulProblem {
            a: arr2(&[[47], [0]]),
            b: arr2(&[[1, 0, 30]]),
            bias: arr2(&[[0]]),
            a0: 86,
            b0: 19,
            c0: 0,
            a_scale: 0.1,
            b_scale: 1.0,
            c_scale: 0.6,
        }
        .check();
    }

    #[test]
    fn rounding_ties_3() {
        QMatMulProblem {
            a: arr2(&[[-30]]),
            b: arr2(&[[0, 107, 0]]),
            bias: arr2(&[[0]]),
            a0: -59,
            b0: 117,
            c0: 0,
            a_scale: 1.0,
            b_scale: 0.15,
            c_scale: 0.6,
        };
    }

    #[test]
    fn onnx_test_matmulinteger() {
        QMatMulProblem {
            a: arr2(&[[11, 7, 3], [10, 6, 2], [9, 5, 1], [8, 4, 0]]),
            b: arr2(&[[1, 4], [2, 5], [3, 6]]),
            bias: arr2(&[[0]]),
            a0: 12,
            b0: 0,
            c0: 0,
            a_scale: 1.0,
            b_scale: 1.0,
            c_scale: 1.0,
        }
        .check()
    }

    #[derive(Debug)]
    struct QMatMulProblem {
        a: Array2<i8>,
        b: Array2<i8>,
        bias: Array2<i32>,
        a0: i8,
        b0: i8,
        c0: i8,
        a_scale: f32,
        b_scale: f32,
        c_scale: f32,
    }

    fn round_ties_to_right(x: f32) -> i32 {
        (x + 0.5).floor() as i32
    }

    impl QMatMulProblem {
        fn check(&self) {
            let r = self.reference();
            let t = self.tract();
            if r.iter().zip(t.iter()).any(|(r, t)| r.max(t) - r.min(t) > 1) {
                panic!("mismatch! refernce: {:?} tract: {:?}", r, t)
            }
        }

        fn reference(&self) -> Array2<i8> {
            let a = self.a.map(|&x| (x as f32 - self.a0 as f32) * self.a_scale);
            let b = self.b.map(|&x| (x as f32 - self.b0 as f32) * self.b_scale);
            let c = a.dot(&b);
            let c = c.map(|&x| round_ties_to_right(x / self.c_scale) + self.c0 as i32);
            c.map(|&x| x.max(-128).min(127) as i8)
        }

        fn tract(&self) -> Array2<i8> {
            let mut model = TypedModel::default();
            let mut inputs = tvec!();
            inputs.push(
                model
                    .add_source(
                        "a",
                        TypedFact::dt_shape(i8::datum_type(), &[self.a.nrows(), self.a.ncols()]),
                    )
                    .unwrap(),
            );
            inputs.push(
                model
                    .add_source(
                        "b",
                        TypedFact::dt_shape(i8::datum_type(), &[self.b.nrows(), self.b.ncols()]),
                    )
                    .unwrap(),
            );
            inputs.push(
                model
                    .add_source(
                        "bias",
                        TypedFact::dt_shape(
                            i32::datum_type(),
                            &[self.bias.nrows(), self.bias.ncols()],
                        ),
                    )
                    .unwrap(),
            );
            inputs.push(model.add_source("a0", TypedFact::scalar::<i8>()).unwrap());
            inputs.push(model.add_source("a_scale", TypedFact::scalar::<f32>()).unwrap());
            inputs.push(model.add_source("b0", TypedFact::scalar::<i8>()).unwrap());
            inputs.push(model.add_source("b_scale", TypedFact::scalar::<f32>()).unwrap());
            inputs.push(model.add_source("c0", TypedFact::scalar::<i8>()).unwrap());
            inputs.push(model.add_source("c_scale", TypedFact::scalar::<f32>()).unwrap());
            let result = model
                .wire_node(
                    "qmm",
                    QMatMul::new(
                        false,
                        false,
                        false,
                        i8::datum_type(),
                        MatMulQParams::all_dynamic(3),
                    ),
                    &inputs,
                )
                .unwrap();
            model.set_output_outlets(&result).unwrap();
            let mut result = model
                .into_runnable()
                .unwrap()
                .run(tvec!(
                    self.a.clone().into_tensor(),
                    self.b.clone().into_tensor(),
                    self.bias.clone().into_tensor(),
                    self.a0.into(),
                    self.a_scale.into(),
                    self.b0.into(),
                    self.b_scale.into(),
                    self.c0.into(),
                    self.c_scale.into(),
                ))
                .unwrap();
            result
                .remove(0)
                .into_tensor()
                .into_array::<i8>()
                .unwrap()
                .into_dimensionality()
                .unwrap()
        }
    }

    fn scale() -> BoxedStrategy<f32> {
        prop_oneof![Just(1.0), (1i32..=20).prop_map(|x| x as f32 / 20.0)].boxed()
    }

    impl Arbitrary for QMatMulProblem {
        type Parameters = ();
        type Strategy = BoxedStrategy<QMatMulProblem>;
        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            (1usize..=4, 1usize..=4, 1usize..=4)
                .prop_flat_map(|(m, k, n)| {
                    (
                        Just((m, k, n)),
                        vec(any::<i8>(), m * k..=m * k),
                        vec(any::<i8>(), k * n..=k * n),
                        any::<i8>(),
                        any::<i8>(),
                        any::<i8>(),
                        scale(),
                        scale(),
                        scale(),
                    )
                })
                .prop_map(|((m, k, n), a, b, a0, b0, c0, a_scale, b_scale, c_scale)| {
                    QMatMulProblem {
                        a: Array2::from_shape_vec((m, k), a).unwrap(),
                        b: Array2::from_shape_vec((k, n), b).unwrap(),
                        bias: arr2(&[[0i32]]),
                        a0,
                        b0,
                        c0,
                        a_scale,
                        b_scale,
                        c_scale,
                    }
                })
                .boxed()
        }
    }
}
