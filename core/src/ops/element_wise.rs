use crate::internal::*;
use downcast_rs::Downcast;
use std::fmt;

pub trait ElementWiseMiniOp: fmt::Debug + objekt::Clone + Send + Sync + 'static + Downcast {
    fn name(&self) -> &'static str;
    fn eval_in_place(&self, t: &mut Tensor) -> TractResult<()>;
}
clone_trait_object!(ElementWiseMiniOp);
downcast_rs::impl_downcast!(ElementWiseMiniOp);

#[derive(Debug, Clone)]
pub struct ElementWiseOp(pub Box<dyn ElementWiseMiniOp>);

impl Op for ElementWiseOp {
    fn name(&self) -> Cow<str> {
        format!("{}", self.0.name()).into()
    }

    canonic!();
    op_as_typed_op!();
    op_as_pulsed_op!();
}

impl StatelessOp for ElementWiseOp {
    fn eval(&self, mut inputs: TVec<Arc<Tensor>>) -> TractResult<TVec<Arc<Tensor>>> {
        let mut t = args_1!(inputs).into_tensor();
        self.0.eval_in_place(&mut t)?;
        Ok(tvec!(t.into_arc_tensor()))
    }
}

impl InferenceRulesOp for ElementWiseOp {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p [TensorProxy],
        outputs: &'p [TensorProxy],
    ) -> InferenceResult {
        check_input_arity(&inputs, 1)?;
        check_output_arity(&outputs, 1)?;
        s.equals(&inputs[0].datum_type, &outputs[0].datum_type)?;
        s.equals(&inputs[0].shape, &outputs[0].shape)?;
        Ok(())
    }
    to_typed!();
    inference_op_as_op!();
}

impl TypedOp for ElementWiseOp {
    fn output_facts(&self, inputs: &[&TypedTensorInfo]) -> TractResult<TVec<TypedTensorInfo>> {
        Ok(tvec!(inputs[0].clone()))
    }

    fn axes_info(&self, model: &TypedModel, node: &TypedNode) -> TractResult<AxesInfo> {
        let a = model.outlet_fact(node.inputs[0])?;
        Ok((0..a.shape.rank()).into_iter().map(|axis| AxisInfo::simple(axis)).collect())
    }


    fn pulsify(
        &self,
        _source: &NormalizedModel,
        node: &NormalizedNode,
        target: &mut PulsedModel,
        mapping: &HashMap<OutletId, OutletId>,
        _pulse: usize,
    ) -> TractResult<TVec<OutletId>> {
        let input = mapping[&node.inputs[0]];
        target.wire_node(&*node.name, self.clone(), &[input])
    }

    typed_op_as_op!();
}

impl PulsedOp for ElementWiseOp {
    fn pulsed_output_facts(&self, inputs: &[&PulsedTensorFact]) -> TractResult<TVec<PulsedTensorFact>> {
        Ok(tvec!(inputs[0].clone()))
    }

    pulsed_op_as_op!();
    pulsed_op_to_typed_op!();
}

#[macro_export]
macro_rules! element_wise {
    ($func:ident, $Op:ident $({$($var: ident : $var_typ: path),*})?, $( [$($typ:ident),*] => $f:expr),*) => {
        #[derive(Debug, Clone)]
        pub struct $Op { $( $(pub $var: $var_typ),* )? }
        impl $crate::ops::element_wise::ElementWiseMiniOp for $Op {
            fn name(&self) -> &'static str {
                stringify!($Op)
            }
            fn eval_in_place(&self, t: &mut Tensor) -> TractResult<()> {
                $(
                    $(if t.datum_type() == $typ::datum_type() {
                        let t: &mut[$typ] = t.as_slice_mut::<$typ>()?;
                        let f: fn(&Self, &mut[$typ]) = $f;
                        f(self, t);
                        return Ok(())
                    }
                    )*
                )*
                bail!("{} does not support {:?}", self.name(), t.datum_type());
            }
        }
        pub fn $func($( $($var: $var_typ),* )?) -> $crate::ops::element_wise::ElementWiseOp {
            $crate::ops::element_wise::ElementWiseOp(Box::new($Op { $( $($var),* )? } ))
        }
    }
}
