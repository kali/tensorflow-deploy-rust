use std::collections::HashMap;
use std::marker::PhantomData;

use analyser::helpers::infer_forward_concrete;
use analyser::TensorFact;
use ndarray::prelude::*;
use ops::{Attr, Op, TensorView};
use tfpb::types::DataType;
use tensor::Datum;
use Result;

pub fn build(pb: &::tfpb::node_def::NodeDef) -> Result<Box<Op>> {
    let begin_mask = pb.get_attr_opt_int("begin_mask")?.unwrap_or(0);
    let end_mask = pb.get_attr_opt_int("end_mask")?.unwrap_or(0);
    let shrink_axis_mask = pb.get_attr_opt_int("shrink_axis_mask")?.unwrap_or(0);
    let datatype = pb.get_attr_datatype("T")?;
    Ok(boxed_new!(StridedSlice(datatype)(begin_mask, end_mask, shrink_axis_mask)))
}

#[derive(Debug, Default, Clone, new)]
pub struct StridedSlice<T:Datum> {
    begin_mask: i64,
    end_mask: i64,
    shrink_axis_mask: i64,
    _phantom: PhantomData<T>,
}

impl<T:Datum> Op for StridedSlice<T> {
    /// Evaluates the operation given the input tensors.
    fn eval(&self, mut inputs: Vec<TensorView>) -> Result<Vec<TensorView>> {
        let (input, begin, end, strides) = args_4!(inputs);
        let input = T::tensor_to_view(&input)?;
        let begin = begin.as_i32s().ok_or("Begin expected as I32")?;
        let end = end.as_i32s().ok_or("End expected as I32")?;
        let strides = strides.as_i32s().ok_or("Strides expected as I32")?;
        struct Dim {
            begin: i32,
            stride: i32,
            len: usize,
            shrink: bool,
        };
        let bounds: Vec<Dim> = (0..input.shape().len())
            .map(|d| {
                let max = input.shape()[d] as i32;
                // deal with too small dim begin/end/stride for input rank
                if d >= begin.len() {
                    return Dim {
                        begin: 0,
                        stride: 1,
                        len: max as usize,
                        shrink: false,
                    };
                }

                // deal with negative indexing
                let b = if begin[d] >= 0 {
                    begin[d]
                } else {
                    max + begin[d]
                };
                let e = if end[d] >= 0 { end[d] } else { max + end[d] };

                // deal with shrinking
                if self.shrink_axis_mask & 1 << d != 0 {
                    return Dim {
                        begin: b,
                        stride: 1,
                        len: 1,
                        shrink: true,
                    };
                }

                // deal with begin and end masks
                let s = strides[d];
                let b = if (self.begin_mask >> d) & 1 == 1 {
                    if s.signum() > 0 {
                        0
                    } else {
                        max - 1
                    }
                } else {
                    b
                };
                let e = if (self.end_mask >> d) & 1 == 1 {
                    if s.signum() < 0 {
                        -1
                    } else {
                        max
                    }
                } else {
                    e
                };
                let len = (((s.abs() as i32 - 1) + (e - b).abs()) / s.abs()) as usize;
                Dim {
                    begin: b,
                    stride: s,
                    len,
                    shrink: false,
                }
            })
            .collect();
        //        println!("input shape: {:?}, bounds: {:?}", input.shape(), bounds);
        let shape: Vec<usize> = bounds.iter().map(|d| d.len).collect();
        let reshape: Vec<usize> = bounds.iter().filter(|d| !d.shrink).map(|d| d.len).collect();
        //        println!("output shape: {:?}", shape);
        let output = Array::from_shape_fn(shape, |coords| {
            let coord: Vec<_> = coords
                .slice()
                .iter()
                .enumerate()
                .map(|(d, i)| (*i as i32 * bounds[d].stride + bounds[d].begin) as usize)
                .collect();
            input[&*coord]
        });
        let output = output.into_shape(reshape)?;

        Ok(vec![T::array_into_tensor(output.into()).into()])
    }

    /// Returns the attributes of the operation and their values.
    fn get_attributes(&self) -> HashMap<&'static str, Attr> {
        hashmap!{
            "begin_mask"       => Attr::I64(self.begin_mask),
            "end_mask"         => Attr::I64(self.end_mask),
            "shrink_axis_mask" => Attr::I64(self.shrink_axis_mask),
        }
    }

    /// Infers properties about the output tensors from the input tensors.
    fn infer_forward(&self, inputs: Vec<&TensorFact>) -> Result<Option<Vec<TensorFact>>> {
        if inputs.len() != 4 {
            bail!("StridedSlice operation only supports four inputs.");
        }

        if let Some(output) = infer_forward_concrete(self, &inputs)? {
            return Ok(Some(output));
        }

        // TODO(liautaud): It will be fun implementing this, I promess.
        Ok(None)
    }

    /// Infers properties about the input tensors from the output tensors.
    fn infer_backward(&self, outputs: Vec<&TensorFact>) -> Result<Option<Vec<TensorFact>>> {
        if outputs.len() < 1 {
            bail!("StridedSlice operation only supports one output.");
        }

        let input = TensorFact {
            datatype: outputs[0].datatype,
            shape: shapefact![..],
            value: valuefact!(_),
        };

        let begin = TensorFact {
            datatype: typefact!(DataType::DT_INT32),
            shape: shapefact![_],
            value: valuefact!(_),
        };

        let end = TensorFact {
            datatype: typefact!(DataType::DT_INT32),
            shape: shapefact![_],
            value: valuefact!(_),
        };

        let strides = TensorFact {
            datatype: typefact!(DataType::DT_INT32),
            shape: shapefact![_],
            value: valuefact!(_),
        };

        Ok(Some(vec![input, begin, end, strides]))
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use ndarray::*;
    use Tensor;

    fn run<I, B, E, S>(op: StridedSlice<i32>, input: I, begin: B, end: E, strides: S) -> Tensor
    where
        I: Into<Tensor>,
        B: Into<Tensor>,
        E: Into<Tensor>,
        S: Into<Tensor>,
    {
        op.eval(vec![
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
    fn strided_slice_1() {
        assert_eq!(
            run(
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
    fn strided_slice_2() {
        assert_eq!(
            run(
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
    fn strided_slice_3() {
        assert_eq!(
            run(
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
    fn strided_slice_4() {
        assert_eq!(
            run(
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
    fn strided_slice_5() {
        assert_eq!(
            run(
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
    fn strided_slice_6() {
        assert_eq!(
            run(
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
    fn strided_slice_7() {
        assert_eq!(
            run(
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
    fn strided_slice_begin_mask_1() {
        let mut op = StridedSlice::default();
        op.begin_mask = 1;
        assert_eq!(
            run(op, arr1(&[0, 1]), arr1(&[1]), arr1(&[1]), arr1(&[1])),
            Tensor::from(arr1(&[0]))
        )
    }

    #[test]
    fn strided_slice_shrink_1() {
        let mut op = StridedSlice::default();
        op.shrink_axis_mask = 1;
        assert_eq!(
            run(
                op,
                arr2(&[[0]]),
                arr1(&[0, 0]),
                arr1(&[0, 0]),
                arr1(&[1, 1])
            ),
            Tensor::I32(arr1(&[]).into_dyn())
        )
    }
}
