use std::iter::repeat;
use std::collections::HashMap;
use std::marker::PhantomData;

use super::{Attr, Op, TensorView};
use analyser::helpers::infer_forward_concrete;
use analyser::{TensorFact, ShapeFact};
use tensor::Datum;
use Result;

pub fn space_to_batch_nd(pb: &::tfpb::node_def::NodeDef) -> Result<Box<Op>> {
    let datatype = pb.get_attr_datatype("T")?;
    Ok(boxed_new!(SpaceToBatch(datatype)()))
}
pub fn batch_to_space_nd(pb: &::tfpb::node_def::NodeDef) -> Result<Box<Op>> {
    let datatype = pb.get_attr_datatype("T")?;
    Ok(boxed_new!(BatchToSpace(datatype)()))
}

#[derive(Debug, new)]
pub struct SpaceToBatch<T: Datum>(PhantomData<T>);

impl<T: Datum> Op for SpaceToBatch<T> {
    /// Evaluates the operation given the input tensors.
    fn eval(&self, mut inputs: Vec<TensorView>) -> Result<Vec<TensorView>> {
        let (input, block_shape, paddings) = args_3!(inputs);
        let block_shape = block_shape.as_i32s().ok_or("block shape expected as I32")?;
        let paddings = paddings.as_i32s().ok_or("paddings expected as I32")?;
        let mut data = T::tensor_into_array(input.into_tensor())?;

        for (ix, pad) in paddings.outer_iter().enumerate() {
            if pad[0] != 0 {
                let mut pad_shape = data.shape().to_vec();
                pad_shape[ix + 1] = pad[0] as usize;
                let tmp = ::ndarray::stack(
                    ::ndarray::Axis(ix + 1),
                    &[::ndarray::ArrayD::zeros(pad_shape).view(), data.view()],
                )?;
                data = tmp;
            }
            if pad[1] != 0 {
                let mut pad_shape = data.shape().to_vec();
                pad_shape[ix + 1] = pad[1] as usize;
                let tmp = ::ndarray::stack(
                    ::ndarray::Axis(ix + 1),
                    &[data.view(), ::ndarray::ArrayD::zeros(pad_shape).view()],
                )?;
                data = tmp;
            }
        }
        let mut reshaped = vec![data.shape()[0]];
        let block_size = block_shape.iter().map(|a| *a as usize).product::<usize>();
        let mut final_shape = vec![block_size * data.shape()[0]];
        for (m, &block_shape_dim) in block_shape.iter().enumerate() {
            reshaped.push(data.shape()[m + 1] / block_shape_dim as usize);
            reshaped.push(block_shape_dim as usize);
            final_shape.push(data.shape()[m + 1] / block_shape_dim as usize);
        }
        reshaped.extend(&data.shape()[block_shape.len() + 1..]);
        final_shape.extend(&data.shape()[block_shape.len() + 1..]);
        let data = data.into_shape(reshaped)?;

        let mut permuted_axes: Vec<_> = (0..block_shape.len()).map(|x| 2 * x + 2).collect();
        permuted_axes.push(0);
        permuted_axes.extend((0..block_shape.len()).map(|x| 2 * x + 1));
        permuted_axes.extend((block_shape.len() * 2 + 1)..data.ndim());
        let data = data.permuted_axes(permuted_axes);
        let data: Vec<T> = data.into_iter().map(|x| *x).collect();
        let data = ::ndarray::ArrayD::from_shape_vec(final_shape, data)?;

        Ok(vec![T::array_into_tensor(data).into()])
    }

    /// Returns the attributes of the operation and their values.
    fn get_attributes(&self) -> HashMap<&'static str, Attr> {
        hashmap!{ "T" => Attr::DataType(T::datatype()) }
    }

    /// Infers properties about the output tensors from the input tensors.
    /// See tensorflow.org/api_docs/cc/class/tensorflow/ops/space-to-batch-n-d.
    fn infer_forward(&self, inputs: Vec<&TensorFact>) -> Result<Option<Vec<TensorFact>>> {
        if inputs.len() != 3 {
            bail!("SpaceToBatchND operation only supports three inputs.");
        }

        if let Some(output) = infer_forward_concrete(self, &inputs)? {
            return Ok(Some(output));
        }

        let input_shape = &inputs[0].shape;
        let mut input_dims = input_shape.dims.clone();

        let block_shape = unwrap_or_none!(inputs[1].value.concretize())
            .as_i32s()
            .ok_or("Expected a i32 matrix for block_shape.")?;

        if block_shape.ndim() != 1 {
            bail!("SpaceToBatchND expected block_shape to be of shape [M].");
        }

        let m = block_shape.shape()[0];

        let paddings = unwrap_or_none!(inputs[2].value.concretize())
            .as_i32s()
            .ok_or("Expected a i32 matrix for paddings.")?;

        if paddings.ndim() != 2 ||
           paddings.shape()[0] != m ||
           paddings.shape()[1] != 2 {
            bail!("SpaceToBatchND expected paddings to be of shape [M, 2].");
        }

        // The input is supposed to have shape [batch] + spatial_shape + remaining_shape,
        // where spatial_shape has M dimensions. If this is not the case but the input
        // shape is open, we can just add unknown dimensions until we get the right shape.
        let input_rank = input_dims.len();
        if input_rank < 1 + m {
            if input_shape.open {
                input_dims.extend(repeat(dimfact!(_)).take(1 + m - input_rank));
            } else {
                bail!("SpaceToBatchND expected the input to have at least 1 + M dimensions.");
            }
        }

        for i in 0..m {
            // Pad the spatial dimensions according to `paddings`.
            input_dims[i + 1] = input_dims[i + 1] + (paddings[[i, 0]] as usize);
            input_dims[i + 1] = input_dims[i + 1] + (paddings[[i, 1]] as usize);

            // Reduce the spatial dimensions according to `block_shape`.
            input_dims[i + 1] = input_dims[i + 1] / (block_shape[[i]] as usize);

            // Expand the batch dimension according to `block_shape`.
            input_dims[0] = input_dims[0] * (block_shape[[i]] as usize);
        }

        let output_shape = if input_shape.open {
            ShapeFact::open(input_dims)
        } else {
            ShapeFact::closed(input_dims)
        };

        let output = TensorFact {
            datatype: inputs[0].datatype,
            shape: output_shape,
            value: valuefact!(_),
        };

        Ok(Some(vec![output]))
    }

    /// Infers properties about the input tensors from the output tensors.
    fn infer_backward(&self, outputs: Vec<&TensorFact>) -> Result<Option<Vec<TensorFact>>> {
        if outputs.len() < 1 {
            bail!("SpaceToBatchND operation only supports one output.");
        }

        let input = TensorFact {
            datatype: outputs[0].datatype,
            shape: shapefact![_; ..],
            value: valuefact!(_),
        };

        let block_shape = TensorFact {
            datatype: typefact!(_),
            shape: shapefact![_],
            value: valuefact!(_),
        };

        let paddings = TensorFact {
            datatype: typefact!(_),
            shape: shapefact![_, 2],
            value: valuefact!(_),
        };

        Ok(Some(vec![input, block_shape, paddings]))
    }
}

#[derive(Debug, new)]
pub struct BatchToSpace<T: Datum>(PhantomData<T>);

impl<T: Datum> Op for BatchToSpace<T> {
    /// Evaluates the operation given the input tensors.
    fn eval(&self, mut inputs: Vec<TensorView>) -> Result<Vec<TensorView>> {
        use ndarray::*;
        let (input, block_shape, crops) = args_3!(inputs);
        let block_shape = block_shape.as_i32s().ok_or("block shape expected as I32")?;
        let crops = crops.as_i32s().ok_or("crops expected as I32")?;
        let data = T::tensor_into_array(input.into_tensor())?;
        let input_shape = data.shape().to_vec();
        let crops = crops.clone().into_shape((block_shape.len(), 2))?;

        let block_size = block_shape.iter().map(|a| *a as usize).product::<usize>();

        // block_dim_1 .. block_dim_n, batches/bloc_size, dim_1, .. dim_n, chan_1, .., chan_n
        let mut unflatten_blocked_shape = vec![];
        unflatten_blocked_shape.extend(block_shape.iter().map(|a| *a as usize));
        let batches = data.shape()[0] / block_size;
        unflatten_blocked_shape.push(batches);
        unflatten_blocked_shape.extend(&data.shape()[1..]);
        let data = data.into_shape(&*unflatten_blocked_shape)?;
        let mut permuted_axes = vec![block_shape.len()];
        let mut padded_shape = vec![batches];
        for i in 0..block_shape.shape()[0] {
            permuted_axes.push(block_shape.len() + 1 + i);
            permuted_axes.push(i);
            padded_shape.push(block_shape[i] as usize * input_shape[i + 1]);
        }
        permuted_axes.extend((1 + block_shape.len() * 2)..data.ndim());
        padded_shape.extend(&input_shape[1 + block_shape.len()..]);
        let data = data.permuted_axes(permuted_axes);
        let data: Vec<T> = data.into_iter().map(|x| *x).collect();
        let data = ::ndarray::ArrayD::from_shape_vec(padded_shape, data)?;
        let mut data = data;
        for (i, crop) in crops.outer_iter().enumerate() {
            if crop[0] != 0 || crop[1] != 0 {
                let end = data.shape()[1 + i] as usize;
                let range = (crop[0] as usize)..(end - crop[1] as usize);
                data = data.slice_axis(Axis(i + 1), range.into())
                    .map(|x| *x)
                    .to_owned();
            }
        }
        Ok(vec![T::array_into_tensor(data).into()])
    }

    /// Returns the attributes of the operation and their values.
    fn get_attributes(&self) -> HashMap<&'static str, Attr> {
        hashmap!{ "T" => Attr::DataType(T::datatype()) }
    }

    /// Infers properties about the output tensors from the input tensors.
    /// See tensorflow.org/api_docs/cc/class/tensorflow/ops/batch-to-space-n-d.
    fn infer_forward(&self, inputs: Vec<&TensorFact>) -> Result<Option<Vec<TensorFact>>> {
        if inputs.len() != 3 {
            bail!("BatchToSpaceND operation only supports three inputs.");
        }

        if let Some(output) = infer_forward_concrete(self, &inputs)? {
            return Ok(Some(output));
        }

        let input_shape = &inputs[0].shape;
        let mut input_dims = input_shape.dims.clone();

        let block_shape = unwrap_or_none!(inputs[1].value.concretize())
            .as_i32s()
            .ok_or("Expected a i32 matrix for block_shape.")?;

        if block_shape.ndim() != 1 {
            bail!("BatchToSpaceND expected block_shape to be of shape [M].");
        }

        let m = block_shape.shape()[0];

        let crops = unwrap_or_none!(inputs[2].value.concretize())
            .as_i32s()
            .ok_or("Expected a i32 matrix for crops.")?;

        if crops.ndim() != 2 ||
           crops.shape()[0] != m ||
           crops.shape()[1] != 2 {
            bail!("BatchToSpaceND expected crops to be of shape [M, 2].");
        }

        // The input is supposed to have shape [batch] + spatial_shape + remaining_shape,
        // where spatial_shape has M dimensions. If this is not the case but the input
        // shape is open, we can just add unknown dimensions until we get the right shape.
        let input_rank = input_dims.len();
        if input_rank < 1 + m {
            if input_shape.open {
                input_dims.extend(repeat(dimfact!(_)).take(1 + m - input_rank));
            } else {
                bail!("BatchToSpaceND expected the input to have at least 1 + M dimensions.");
            }
        }

        for i in 0..m {
            // Expand the spatial dimensions according to `block_shape`.
            input_dims[i + 1] = input_dims[i + 1] * (block_shape[[i]] as usize);

            // Reduce the batch dimension according to `block_shape`.
            input_dims[0] = input_dims[0] / (block_shape[[i]] as usize);

            // Crop the spatial dimensions according to `crops`.
            input_dims[i + 1] = input_dims[i + 1] - (crops[[i, 0]] as usize);
            input_dims[i + 1] = input_dims[i + 1] - (crops[[i, 1]] as usize);
        }

        let output_shape = if input_shape.open {
            ShapeFact::open(input_dims)
        } else {
            ShapeFact::closed(input_dims)
        };

        let output = TensorFact {
            datatype: inputs[0].datatype,
            shape: output_shape,
            value: valuefact!(_),
        };

        Ok(Some(vec![output]))
    }

    /// Infers properties about the input tensors from the output tensors.
    fn infer_backward(&self, outputs: Vec<&TensorFact>) -> Result<Option<Vec<TensorFact>>> {
        if outputs.len() < 1 {
            bail!("BatchToSpaceND operation only supports one output.");
        }

        let input = TensorFact {
            datatype: outputs[0].datatype,
            shape: shapefact![_; ..],
            value: valuefact!(_),
        };

        let block_shape = TensorFact {
            datatype: typefact!(_),
            shape: shapefact![_],
            value: valuefact!(_),
        };

        let crops = TensorFact {
            datatype: typefact!(_),
            shape: shapefact![_, 2],
            value: valuefact!(_),
        };

        Ok(Some(vec![input, block_shape, crops]))
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use ndarray::*;
    use ops::nn::arr4;

    // https://www.tensorflow.org/api_docs/python/tf/space_to_batch_nd
    #[test]
    fn space_to_batch_nd_1() {
        assert_eq!(
            SpaceToBatch::<i32>::new()
                .eval(vec![
                    arr4(&[[[[1], [2]], [[3], [4]]]]).into(),
                    arr1(&[2, 2]).into(),
                    arr2(&[[0, 0], [0, 0]]).into(),
                ])
                .unwrap(),
            vec![arr4(&[[[[1i32]]], [[[2]]], [[[3]]], [[[4]]]]).into()],
        )
    }

    #[test]
    fn space_to_batch_nd_2() {
        assert_eq!(
            SpaceToBatch::<i32>::new()
                .eval(vec![
                    arr4(&[[[[1, 2, 3], [4, 5, 6]], [[7, 8, 9], [10, 11, 12]]]]).into(),
                    arr1(&[2, 2]).into(),
                    arr2(&[[0, 0], [0, 0]]).into(),
                ])
                .unwrap(),
            vec![
                arr4(&[
                    [[[1i32, 2, 3]]],
                    [[[4, 5, 6]]],
                    [[[7, 8, 9]]],
                    [[[10, 11, 12]]],
                ]).into(),
            ],
        )
    }

    #[test]
    fn space_to_batch_nd_3() {
        assert_eq!(
            SpaceToBatch::<i32>::new()
                .eval(vec![
                    arr4(&[[
                        [[1], [2], [3], [4]],
                        [[5], [6], [7], [8]],
                        [[9], [10], [11], [12]],
                        [[13], [14], [15], [16]],
                    ]]).into(),
                    arr1(&[2, 2]).into(),
                    arr2(&[[0, 0], [0, 0]]).into(),
                ])
                .unwrap(),
            vec![
                arr4(&[
                    [[[1], [3]], [[9], [11]]],
                    [[[2], [4]], [[10], [12]]],
                    [[[5], [7]], [[13], [15]]],
                    [[[6], [8]], [[14], [16]]],
                ]).into(),
            ],
        )
    }

    #[test]
    fn space_to_batch_nd_4() {
        assert_eq!(
            SpaceToBatch::<i32>::new()
                .eval(vec![
                    arr4(&[
                        [[[1], [2], [3], [4]], [[5], [6], [7], [8]]],
                        [[[9], [10], [11], [12]], [[13], [14], [15], [16]]],
                    ]).into(),
                    arr1(&[2, 2]).into(),
                    arr2(&[[0, 0], [2, 0]]).into(),
                ])
                .unwrap(),
            vec![
                arr4(&[
                    [[[0], [1], [3]]],
                    [[[0], [9], [11]]],
                    [[[0], [2], [4]]],
                    [[[0], [10], [12]]],
                    [[[0], [5], [7]]],
                    [[[0], [13], [15]]],
                    [[[0], [6], [8]]],
                    [[[0], [14], [16]]],
                ]).into(),
            ],
        )
    }

    #[test]
    fn batch_to_space_nd_1() {
        assert_eq!(
            BatchToSpace::<i32>::new()
                .eval(vec![
                    arr4(&[[[[1]]], [[[2]]], [[[3]]], [[[4]]]]).into(),
                    arr1(&[2, 2]).into(),
                    arr2(&[[0, 0], [0, 0]]).into(),
                ])
                .unwrap(),
            vec![arr4(&[[[[1], [2]], [[3], [4]]]]).into()]
        )
    }

    #[test]
    fn batch_to_space_nd_2() {
        assert_eq!(
            BatchToSpace::<i32>::new()
                .eval(vec![
                    arr4(&[
                        [[[1i32, 2, 3]]],
                        [[[4, 5, 6]]],
                        [[[7, 8, 9]]],
                        [[[10, 11, 12]]],
                    ]).into(),
                    arr1(&[2, 2]).into(),
                    arr2(&[[0, 0], [0, 0]]).into(),
                ])
                .unwrap(),
            vec![arr4(&[[[[1i32, 2, 3], [4, 5, 6]], [[7, 8, 9], [10, 11, 12]]]]).into()]
        )
    }

    #[test]
    fn batch_to_space_nd_3() {
        assert_eq!(
            BatchToSpace::<i32>::new()
                .eval(vec![
                    arr4(&[
                        [[[1i32], [3]], [[9], [11]]],
                        [[[2], [4]], [[10], [12]]],
                        [[[5], [7]], [[13], [15]]],
                        [[[6], [8]], [[14], [16]]],
                    ]).into(),
                    arr1(&[2, 2]).into(),
                    arr2(&[[0, 0], [0, 0]]).into(),
                ])
                .unwrap(),
            vec![
                arr4(&[[
                    [[1i32], [2], [3], [4]],
                    [[5], [6], [7], [8]],
                    [[9], [10], [11], [12]],
                    [[13], [14], [15], [16]],
                ]]).into(),
            ]
        )
    }

    #[test]
    fn batch_to_space_nd_4() {
        assert_eq!(
            BatchToSpace::<i32>::new()
                .eval(vec![
                    arr4(&[
                        [[[0i32], [1], [3]]],
                        [[[0], [9], [11]]],
                        [[[0], [2], [4]]],
                        [[[0], [10], [12]]],
                        [[[0], [5], [7]]],
                        [[[0], [13], [15]]],
                        [[[0], [6], [8]]],
                        [[[0], [14], [16]]],
                    ]).into(),
                    arr1(&[2, 2]).into(),
                    arr2(&[[0, 0], [2, 0]]).into(),
                ])
                .unwrap(),
            vec![
                arr4(&[
                    [[[1], [2], [3], [4]], [[5], [6], [7], [8]]],
                    [[[9], [10], [11], [12]], [[13], [14], [15], [16]]],
                ]).into(),
            ]
        )
    }
}
