use std::marker::PhantomData;

use tfdeploy::analyser::prelude::*;
use tfdeploy::analyser::rules::prelude::*;
use tfdeploy::ops::prelude::*;
use ndarray::prelude::*;
use tfdeploy::TfdResult;

pub fn build(pb: &::tfpb::node_def::NodeDef) -> TfdResult<Box<Op>> {
    let begin_mask = pb.get_attr_opt_int("begin_mask")?.unwrap_or(0);
    let end_mask = pb.get_attr_opt_int("end_mask")?.unwrap_or(0);
    let shrink_axis_mask = pb.get_attr_opt_int("shrink_axis_mask")?.unwrap_or(0);
    let datum_type = pb.get_attr_datum_type("T")?;
    let base = BaseStridedSlice::new(begin_mask, end_mask, shrink_axis_mask);
    if datum_type == DatumType::I32 {
        Ok(Box::new(StridedSliceD::new(base)))
    } else {
        Ok(boxed_new!(StridedSlice(datum_type)(base)))
    }
}

#[derive(Debug, Clone)]
struct StrideSliceBuffer {
    skip: Option<usize>,
}
impl OpBuffer for StrideSliceBuffer {}

#[derive(Debug, Default, Clone, new)]
pub struct BaseStridedSlice {
    begin_mask: i64,
    end_mask: i64,
    shrink_axis_mask: i64,
}

#[derive(Debug, Clone)]
struct Dim {
    begin: TDim,
    end: TDim,
    stride: i32,
    shrink: bool,
}

impl Dim {
    fn len(&self) -> TfdResult<usize> {
        Ok(
            (((self.stride.abs() as i32 - 1) + (self.end - self.begin).to_integer()?.abs() as i32)
                / self.stride.abs()) as usize,
        )
    }

    fn soft_len(&self) -> TfdResult<TDim> {
        if let Ok(len) = (self.end - self.begin).to_integer() {
            Ok((((self.stride.abs() as i32 - 1) + len.abs() as i32) / self.stride.abs()).to_dim())
        } else if self.stride == 1 {
            Ok(self.end - self.begin)
        } else {
            bail!("Streaming dimensions with strides are not supported for now")
        }
    }
}

impl BaseStridedSlice {
    fn must_shrink(&self, ix: usize) -> bool {
        self.shrink_axis_mask & (1 << ix) != 0
    }
    fn ignore_begin(&self, ix: usize) -> bool {
        self.begin_mask & (1 << ix) != 0
    }
    fn ignore_end(&self, ix: usize) -> bool {
        self.end_mask & (1 << ix) != 0
    }
    fn prepare_one_dim(
        &self,
        ix: usize,
        dim: TDim,
        begin: &ArrayView1<TDim>,
        end: &ArrayView1<TDim>,
        strides: &ArrayView1<i32>,
    ) -> Dim {
        // deal with too small dim begin/end/stride for input rank
        if ix >= begin.len() {
            return Dim {
                begin: 0.to_dim(),
                end: dim,
                stride: 1,
                shrink: false,
            };
        }

        // deal with negative indexing
        fn must_add_to_len(bound: TDim) -> bool {
            if let Some(b) = bound.as_const() {
                b < 0
            } else {
                bound.eval(100_000_000).unwrap() < 0 // FIXME
            }
        }
        let b: TDim = if must_add_to_len(begin[ix]) {
            dim + begin[ix]
        } else {
            begin[ix]
        };
        let e: TDim = if must_add_to_len(end[ix]) {
            dim + end[ix]
        } else {
            end[ix]
        };

        // deal with shrinking
        if self.must_shrink(ix) {
            return Dim {
                begin: b,
                end: b + 1,
                stride: 1,
                shrink: true,
            };
        }

        // deal with begin and end masks
        let s = strides[ix];
        let b = if self.ignore_begin(ix) {
            if s.signum() > 0 {
                0.to_dim()
            } else {
                dim - 1
            }
        } else {
            b
        };
        let e = if self.ignore_end(ix) {
            if s.signum() < 0 {
                -1.to_dim()
            } else {
                dim
            }
        } else {
            e
        };
        Dim {
            begin: b,
            end: e,
            stride: s,
            shrink: false,
        }
    }

    fn prepare(
        &self,
        input_shape: &[usize],
        begin: Value,
        end: Value,
        strides: Value,
    ) -> TfdResult<(Vec<Dim>, Vec<usize>, Vec<usize>)> {
        let casted_begin = TDim::tensor_cast_to_array(&begin)?;
        let begin = casted_begin.view().into_dimensionality()?;
        let casted_end = TDim::tensor_cast_to_array(&end)?;
        let end = casted_end.view().into_dimensionality()?;
        let strides = strides
            .as_i32s()
            .ok_or("Strides expected as I32")?
            .view()
            .into_dimensionality()?;
        trace!(
            "StridedSlice {:?} computing shapes: input_shape:{:?} begin:{:?} end:{:?} strides:{:?}",
            self,
            input_shape,
            begin,
            end,
            strides
        );
        let bounds: Vec<Dim> = (0..input_shape.len())
            .map(|ix| self.prepare_one_dim(ix, input_shape[ix].to_dim(), &begin, &end, &strides))
            .collect();
        trace!("StridedSlice bounds {:?}", bounds);
        let mid_shape: Vec<usize> = bounds
            .iter()
            .map(|d| d.len())
            .collect::<TfdResult<Vec<usize>>>()?;
        let end_shape: Vec<usize> = bounds
            .iter()
            .filter(|d| !d.shrink)
            .map(|d| d.len())
            .collect::<TfdResult<Vec<usize>>>()?;
        Ok((bounds, mid_shape, end_shape))
    }

    fn eval<T: Datum>(&self, mut inputs: TVec<Value>) -> TfdResult<TVec<Value>> {
        let (input, begin, end, strides) = args_4!(inputs);
        let (bounds, mid_shape, end_shape) = self.prepare(input.shape(), begin, end, strides)?;
        let input = input.to_array_view::<T>()?;
        let output = Array::from_shape_fn(mid_shape, |coords| {
            let coord: Vec<_> = coords
                .slice()
                .iter()
                .enumerate()
                .map(|(d, i)| {
                    (*i as i32 * bounds[d].stride + bounds[d].begin.to_integer().unwrap() as i32)
                        as usize
                })
                .collect();
            input[&*coord]
        });
        let output = output.into_shape(end_shape)?;
        Ok(tvec![output.into()])
    }

    fn step<T: Datum>(
        &self,
        mut inputs: TVec<StepValue>,
        _buffer: &mut Box<OpBuffer>,
    ) -> TfdResult<Option<TVec<Value>>> {
        let (input, begin, end, strides) = args_4!(inputs);

        let begin = begin.into_const().ok_or("begin can not be streamed")?;
        let casted_begin = TDim::tensor_cast_to_array(&begin)?;
        let mut begin: Array1<_> = casted_begin.view().into_dimensionality()?.to_owned();

        let end = end.into_const().ok_or("end can not be streamed")?;
        let casted_end = TDim::tensor_cast_to_array(&end)?;
        let mut end: Array1<_> = casted_end.view().into_dimensionality()?.to_owned();

        let strides = strides.into_const().ok_or("strides can not be streamed")?;
        let strides = strides
            .into_tensor()
            .take_i32s()
            .ok_or("Strides expected as I32")?;
        let mut strides: Array1<_> = strides.into_dimensionality()?;

        let stream = input.into_stream().ok_or("data must be streamed")?;
        let dim = stream.info.axis;

        let input = if let Some(input) = stream.chunk {
            input
        } else {
            return Ok(None);
        };

        if input.shape()[dim] != 1 {
            bail!("StridedSlice assumes streaming chunk of 1")
        }
        let bounds = self.prepare_one_dim(
            dim,
            stream.info.len,
            &begin.view(),
            &end.view(),
            &strides.view(),
        );

        if stream.offset < bounds.begin.to_integer()? as u64 {
            return Ok(None);
        }

        begin[dim] = 0.to_dim();
        end[dim] = 1.to_dim();
        strides[dim] = 1;
        Ok(Some(self.eval::<T>(tvec!(
            input,
            begin.to_owned().into(),
            end.into(),
            strides.into()
        ))?))
    }

    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        solver: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) {
        solver
            .equals(&inputs.len, 4)
            .equals(&outputs.len, 1)
            .equals(&inputs[0].datum_type, &outputs[0].datum_type)
            .equals(&inputs[1].rank, 1)
            .equals(&inputs[2].rank, 1)
            .equals(&inputs[3].rank, 1)
            .equals_all(wrap!(
                &inputs[1].shape[0],
                &inputs[2].shape[0],
                &inputs[3].shape[0]
            ))
            .given(&inputs[0].shape, move |solver, input_shape: Vec<TDim>| {
                solver.given(&inputs[1].value, move |solver, begin: Tensor| {
                    let input_shape = input_shape.clone();
                    solver.given(&inputs[2].value, move |solver, end: Tensor| {
                        let input_shape = input_shape.clone();
                        let begin = begin.clone();
                        solver.given(&inputs[3].value, move |solver, stride: Tensor| {
                            let casted_begin =TDim::tensor_cast_to_array(&begin).unwrap();
                            let begin = casted_begin.view().into_dimensionality().unwrap();
                            let casted_end =TDim::tensor_cast_to_array(&end).unwrap();
                            let end = casted_end.view().into_dimensionality().unwrap();
                            let stride = stride
                                .as_i32s()
                                .unwrap()
                                .view()
                                .into_dimensionality()
                                .unwrap();
                            let mut current_out_dim = 0;
                            for (ix, d) in input_shape.iter().enumerate() {
                                if !self.must_shrink(ix) {
                                    let preped =
                                        self.prepare_one_dim(ix, *d, &begin, &end, &stride);
                                    match preped.soft_len() {
                                        Ok(l) => {
                                            solver.equals(&outputs[0].shape[current_out_dim], l);
                                        }
                                        Err(e) => warn!("Strided slice inference failure: {:?}", e),
                                    }
                                    current_out_dim += 1;
                                }
                            }
                            solver.equals(&outputs[0].rank, current_out_dim as i64);
                        });
                    });
                });
            });
    }

    fn final_prep(
        &self,
        mut inputs: TVec<TensorFact>,
        _outputs: TVec<TensorFact>,
    ) -> TfdResult<Option<Box<Op>>> {
        let (input, begin, end, strides) = args_4!(inputs);
        if let (Some(shape), Some(begin), Some(end), Some(strides)) = (
            input.shape.concretize(),
            begin.concretize(),
            end.concretize(),
            strides.concretize(),
        ) {
            let casted_begin =TDim::tensor_cast_to_array(&begin)?;
            let begin = casted_begin.view().into_dimensionality()?;
            let casted_end =TDim::tensor_cast_to_array(&end)?;
            let end = casted_end.view().into_dimensionality().unwrap();
            let casted_strides = i32::tensor_cast_to_array(&strides)?;
            let strides = casted_strides.view().into_dimensionality()?;

            let bounds: Vec<Dim> = (0..shape.len())
                .map(|ix| self.prepare_one_dim(ix, shape[ix], &begin, &end, &strides))
                .collect::<Vec<_>>();

            if shape.iter().zip(bounds.iter()).all(|(s, b)| {
                s.is_stream()
                    || (!b.shrink
                        && b.begin.to_integer().unwrap() == 0
                        && (b.end.to_integer().unwrap() == 0 || b.end == *s)
                        && b.stride == 1)
            }) {
                if let Some(axis) = shape.iter().position(|d| d.is_stream()) {
                    return Ok(Some(Box::new(SkipBeginStreamStridedSlice::new(
                        bounds[axis].begin.to_integer().unwrap() as u64,
                    ))));
                }
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Default, Clone, new)]
pub struct StridedSlice<T: Datum> {
    base: BaseStridedSlice,
    _phantom: PhantomData<T>,
}

impl<T: Datum> Op for StridedSlice<T> {
    /// Evaluates the operation given the input tensors.
    fn eval(&self, inputs: TVec<Value>) -> TfdResult<TVec<Value>> {
        self.base.eval::<T>(inputs)
    }

    fn step(
        &self,
        inputs: TVec<StepValue>,
        buffer: &mut Box<OpBuffer>,
    ) -> TfdResult<Option<TVec<Value>>> {
        self.base.step::<T>(inputs, buffer)
    }

    fn final_prep(
        &self,
        inputs: TVec<TensorFact>,
        outputs: TVec<TensorFact>,
    ) -> TfdResult<Option<Box<Op>>> {
        self.base.final_prep(inputs, outputs)
    }
}

impl<T: Datum> InferenceRulesOp for StridedSlice<T> {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        solver: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) {
        self.base.rules(solver, inputs, outputs)
    }
}

#[derive(Debug, Default, Clone, new)]
pub struct StridedSliceD {
    base: BaseStridedSlice,
}

impl Op for StridedSliceD {
    /// Evaluates the operation given the input tensors.
    fn eval(&self, inputs: TVec<Value>) -> TfdResult<TVec<Value>> {
        let dt = inputs[0].datum_type();
        match dt {
            DatumType::TDim => self.base.eval::<TDim>(inputs),
            DatumType::I32 => self.base.eval::<i32>(inputs),
            _ => panic!("StridedSliceD only covering i32 and Dim"),
        }
    }

    fn step(
        &self,
        inputs: TVec<StepValue>,
        buffer: &mut Box<OpBuffer>,
    ) -> TfdResult<Option<TVec<Value>>> {
        let dt = inputs[0]
            .as_stream()
            .and_then(|s| s.chunk.as_ref())
            .map(|t| t.datum_type());
        if let Some(dt) = dt {
            match dt {
                DatumType::TDim => self.base.step::<TDim>(inputs, buffer),
                DatumType::I32 => self.base.step::<i32>(inputs, buffer),
                _ => panic!("StridedSliceD only covering i32 and Dim"),
            }
        } else {
            Ok(None)
        }
    }

    fn final_prep(
        &self,
        inputs: TVec<TensorFact>,
        outputs: TVec<TensorFact>,
    ) -> TfdResult<Option<Box<Op>>> {
        self.base.final_prep(inputs, outputs)
    }
}

impl InferenceRulesOp for StridedSliceD {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        solver: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) {
        self.base.rules(solver, inputs, outputs)
    }
}

#[derive(Debug, Default, Clone, new)]
pub struct SkipBeginStreamStridedSlice {
    skip: u64,
}

impl Op for SkipBeginStreamStridedSlice {
    fn step(
        &self,
        mut inputs: TVec<StepValue>,
        _buffer: &mut Box<OpBuffer>,
    ) -> TfdResult<Option<TVec<Value>>> {
        let Stream { offset, chunk, .. } = inputs
            .remove(0)
            .into_stream()
            .ok_or("Input 0 expected to be a stream")?;
        if offset < self.skip {
            Ok(None)
        } else {
            Ok(chunk.map(|d| tvec!(d)))
        }
    }
}

impl InferenceOp for SkipBeginStreamStridedSlice {
    fn infer(
        &self,
        _inputs: TVec<TensorFact>,
        _outputs: TVec<TensorFact>,
    ) -> TfdResult<(TVec<TensorFact>, TVec<TensorFact>)> {
        panic!();
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use ndarray::*;
    use tfdeploy::Tensor;
    use tfdeploy::ops::InferenceOp;

    fn eval<I, B, E, S>(op: StridedSlice<i32>, input: I, begin: B, end: E, strides: S) -> Tensor
    where
        I: Into<Tensor>,
        B: Into<Tensor>,
        E: Into<Tensor>,
        S: Into<Tensor>,
    {
        op.eval(tvec![
            input.into().into(),
            begin.into().into(),
            end.into().into(),
            strides.into().into(),
        ]).unwrap()
            .pop()
            .unwrap()
            .into_tensor()
    }

    // https://www.tensorflow.org/api_docs/python/tf/strided_slice
    #[test]
    fn eval_1() {
        assert_eq!(
            eval(
                StridedSlice::default(),
                arr3(&[
                    [[1, 1, 1], [2, 2, 2]],
                    [[3, 3, 3], [4, 4, 4]],
                    [[5, 5, 5], [6, 6, 6]],
                ]),
                arr1(&[1, 0, 0]),
                arr1(&[2, 1, 3]),
                arr1(&[1, 1, 1])
            ),
            Tensor::from(arr3(&[[[3, 3, 3]]])),
        );
    }

    #[test]
    fn eval_2() {
        assert_eq!(
            eval(
                StridedSlice::default(),
                arr3(&[
                    [[1, 1, 1], [2, 2, 2]],
                    [[3, 3, 3], [4, 4, 4]],
                    [[5, 5, 5], [6, 6, 6]],
                ]),
                arr1(&[1, 0, 0]),
                arr1(&[2, 2, 3]),
                arr1(&[1, 1, 1])
            ),
            Tensor::from(arr3(&[[[3, 3, 3], [4, 4, 4]]])),
        );
    }

    #[test]
    fn eval_3() {
        assert_eq!(
            eval(
                StridedSlice::default(),
                arr3(&[
                    [[1, 1, 1], [2, 2, 2]],
                    [[3, 3, 3], [4, 4, 4]],
                    [[5, 5, 5], [6, 6, 6]],
                ]),
                arr1(&[1, -1, 0]),
                arr1(&[2, -3, 3]),
                arr1(&[1, -1, 1])
            ),
            Tensor::from(arr3(&[[[4, 4, 4], [3, 3, 3]]])),
        );
    }

    #[test]
    fn eval_4() {
        assert_eq!(
            eval(
                StridedSlice::default(),
                arr3(&[
                    [[1, 1, 1], [2, 2, 2]],
                    [[3, 3, 3], [4, 4, 4]],
                    [[5, 5, 5], [6, 6, 6]],
                ]),
                arr1(&[1, 0, 0]),
                arr1(&[2, 2, 4]),
                arr1(&[1, 1, 2])
            ),
            Tensor::from(arr3(&[[[3, 3], [4, 4]]])),
        );
    }

    #[test]
    fn eval_5() {
        assert_eq!(
            eval(
                StridedSlice::default(),
                arr1(&[0, 0]),
                arr1(&[0]),
                arr1(&[-1]),
                arr1(&[1])
            ),
            Tensor::from(arr1(&[0]))
        )
    }

    #[test]
    fn eval_6() {
        assert_eq!(
            eval(
                StridedSlice::default(),
                arr2(&[[1, 0, 0, 0], [3, 0, 0, 0], [0, 0, 0, 0]]),
                arr1(&[-3, -4]),
                arr1(&[-1, -1]),
                arr1(&[1, 2])
            ),
            Tensor::from(arr2(&[[1, 0], [3, 0]]))
        )
    }

    #[test]
    fn eval_7() {
        assert_eq!(
            eval(
                StridedSlice::default(),
                arr2(&[[0, 6], [0, 0]]),
                arr1(&[0]),
                arr1(&[2]),
                arr1(&[1])
            ),
            Tensor::from(arr2(&[[0, 6], [0, 0]]))
        )
    }

    #[test]
    fn eval_begin_mask_1() {
        let mut op = StridedSlice::default();
        op.base.begin_mask = 1;
        assert_eq!(
            eval(op, arr1(&[0, 1]), arr1(&[1]), arr1(&[1]), arr1(&[1])),
            Tensor::from(arr1(&[0]))
        )
    }

    #[test]
    fn eval_shrink_1() {
        let mut op = StridedSlice::default();
        op.base.shrink_axis_mask = 1;
        assert_eq!(
            eval(
                op,
                arr2(&[[0]]),
                arr1(&[0, 0]),
                arr1(&[0, 0]),
                arr1(&[1, 1])
            ),
            Tensor::I32(arr1(&[]).into_dyn())
        )
    }

    #[test]
    fn eval_shrink_to_scalar() {
        let mut op = StridedSlice::default();
        op.base.shrink_axis_mask = 1;
        assert_eq!(
            eval(op, arr1(&[0]), arr1(&[0]), arr1(&[0]), arr1(&[1])),
            Tensor::I32(arr0(0).into_dyn())
        )
    }

    #[test]
    fn inference_1() {
        let op = StridedSlice::<f32>::new(BaseStridedSlice::new(5, 7, 0));
        let input = TensorFact::default().with_datum_type(DatumType::F32);
        let begin = TensorFact::from(arr1(&[0i32, 2, 0]));
        let end = TensorFact::from(arr1(&[0i32, 0, 0]));
        let strides = TensorFact::from(arr1(&[1i32, 1, 1]));

        let (input_facts, output_facts) =
            op.infer(
                tvec![input, begin.clone(), end.clone(), strides.clone()],
                tvec![TensorFact::default()],
            ).unwrap();
        assert_eq!(
            input_facts,
            tvec![
                TensorFact::default()
                    .with_datum_type(DatumType::F32)
                    .with_shape(shapefact![..]),
                begin,
                end,
                strides,
            ]
        );
        assert_eq!(
            output_facts,
            tvec![
                TensorFact::default()
                    .with_datum_type(DatumType::F32)
                    .with_shape(shapefact![..]),
            ]
        );
    }

    #[test]
    fn inference_2() {
        let op = StridedSlice::<f32>::new(BaseStridedSlice::new(1, 1, 2));
        let input = TensorFact::default().with_datum_type(DatumType::F32);
        let begin = TensorFact::from(arr1(&[0i32, 0]));
        let end = TensorFact::from(arr1(&[0i32, 1]));
        let strides = TensorFact::from(arr1(&[1i32, 1]));

        let (input_facts, output_facts) =
            op.infer(
                tvec![input, begin.clone(), end.clone(), strides.clone()],
                tvec![TensorFact::default()],
            ).unwrap();
        assert_eq!(
            input_facts,
            tvec![
                TensorFact::default()
                    .with_datum_type(DatumType::F32)
                    .with_shape(shapefact![..]),
                begin,
                end,
                strides,
            ]
        );
        assert_eq!(
            output_facts,
            tvec![
                TensorFact::default()
                    .with_datum_type(DatumType::F32)
                    .with_shape(shapefact![..]),
            ]
        );
    }

    #[test]
    fn inference_3() {
        let op = StridedSlice::<f32>::new(BaseStridedSlice::new(5, 7, 0));
        let input = TensorFact::dt_shape(DatumType::F32, shapefact!(1, (TDim::stream() - 2), 16));
        let begin = TensorFact::from(arr1(&[0i32, 2, 0]));
        let end = TensorFact::from(arr1(&[0i32, 0, 0]));
        let strides = TensorFact::from(arr1(&[1i32, 1, 1]));

        let (_, output_facts) =
            op.infer(
                tvec![input, begin, end, strides],
                tvec![TensorFact::default()],
            ).unwrap();

        assert_eq!(
            output_facts,
            tvec![TensorFact::dt_shape(
                DatumType::F32,
                shapefact!(1, (TDim::stream() - 4), 16)
            )]
        );
    }

    #[test]
    fn inference_4() {
        let op = StridedSlice::<f32>::new(BaseStridedSlice::new(5, 7, 0));
        let input = TensorFact::dt_shape(DatumType::F32, shapefact!(1, (TDim::stream() - 2), 16));
        let begin = TensorFact::from(arr1(&[0i32, 2, 0]));
        let end = TensorFact::from(arr1(&[0i32, 0, 0]));
        let strides = TensorFact::from(arr1(&[1i32, 1, 1]));

        let (_, output_facts) =
            op.infer(
                tvec![input, begin, end, strides],
                tvec![TensorFact::default()],
            ).unwrap();

        assert_eq!(
            output_facts,
            tvec![TensorFact::dt_shape(
                DatumType::F32,
                shapefact!(1, (TDim::stream() - 4), 16)
            )]
        );
    }
}
