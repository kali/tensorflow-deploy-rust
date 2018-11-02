#![cfg(feature="conform")]
#![allow(non_snake_case)]
#[macro_use]
extern crate log;
extern crate ndarray;
extern crate pretty_env_logger;
#[macro_use]
extern crate proptest;
extern crate protobuf;
extern crate tensorflow;
#[macro_use]
extern crate tract_core;
extern crate tract_tensorflow;

mod utils;

use tract_tensorflow::conform::*;
use ndarray::prelude::*;
use proptest::prelude::*;
use protobuf::Message;
use tract_core::ops::StatefullOp;
use tract_core::tensor::arr4;
use tract_core::tensor::Datum;
use tract_core::Tensor as TractTensor;
use tract_tensorflow::tfpb;
use tract_tensorflow::tfpb::types::DataType::DT_FLOAT;
use tract_tensorflow::tfpb::types::DataType::DT_INT32;
use utils::*;

fn space_to_batch_strat() -> BoxedStrategy<(TractTensor, TractTensor, TractTensor)> {
    use proptest::collection::vec;
    (
        1usize..4,
        vec(1usize..8, 1usize..4),
        vec(1usize..8, 1usize..4),
    )
        .prop_flat_map(|(b, spatial_dims, non_spatial_dims)| {
            (
                Just(b),
                Just(spatial_dims.clone()),
                Just(non_spatial_dims),
                vec(1usize..4, spatial_dims.len()..spatial_dims.len() + 1),
                vec(0usize..4, spatial_dims.len()..spatial_dims.len() + 1),
            )
        }).prop_filter("block < input", |&(_, ref sd, _, ref bs, _)| {
            bs.iter().zip(sd.iter()).all(|(bs, is)| bs <= is)
        }).prop_map(
            |(b, sd, nsd, bs, left_pad): (
                usize,
                Vec<usize>,
                Vec<usize>,
                Vec<usize>,
                Vec<usize>,
            )| {
                let mut input_shape = vec![b];
                input_shape.extend(&sd);
                input_shape.extend(&nsd);
                let input = ArrayD::from_shape_vec(
                    input_shape.clone(),
                    (0..input_shape.iter().cloned().product())
                        .map(|i| (1 + i) as f32)
                        .collect(),
                ).unwrap();
                let block_size = Array1::from_shape_fn(sd.len(), |i| bs[i] as i32).into_dyn();
                let padding = Array2::<i32>::from_shape_fn((sd.len(), 2), |(d, locus)| {
                    (if locus == 0 {
                        left_pad[d] as i32
                    } else {
                        block_size[d] - (sd[d] + left_pad[d]) as i32 % block_size[d]
                    })
                });
                (input.into(), block_size.into(), padding.into_dyn().into())
            },
        ).boxed()
}

proptest! {
    #[test]
    fn space_to_batch((ref i, ref bs, ref p) in space_to_batch_strat()) {
        let graph = tfpb::graph()
            .node(placeholder_f32("input"))
            .node(placeholder("block_shape", DT_INT32, tensor_shape(bs.shape())))
            .node(placeholder_i32("paddings"))
            .node(tfpb::node().name("op").op("SpaceToBatchND").input("input")
            .input("block_shape")
            .input("paddings")
            .attr("T", DT_FLOAT)
            );
        let graph = graph.write_to_bytes()?;
        let inputs = vec!(("input", i.clone()), ("block_shape", bs.clone()), ("paddings", p.clone()));
        compare(&graph, inputs, "op")?
    }
}

fn batch_to_space_strat() -> BoxedStrategy<(TractTensor, TractTensor, TractTensor)> {
    space_to_batch_strat()
        .prop_map(|(i, bs, p)| {
            let batches: TractTensor =
                tract_tensorflow::ops::nn::s2b::raw::SpaceToBatch::new(f32::datum_type())
                    .as_stateless()
                    .unwrap()
                    .eval(tvec![i.into(), bs.clone().into(), p.clone().into()])
                    .unwrap()
                    .remove(0)
                    .into_tensor();
            (batches, bs, p)
        }).boxed()
}

proptest! {
    #[test]
    fn batch_to_space((ref b, ref bs, ref c) in batch_to_space_strat()) {
        let graph = tfpb::graph()
            .node(placeholder_f32("input"))
            .node(placeholder("block_shape", DT_INT32, tensor_shape(bs.shape())))
            .node(placeholder_i32("crops"))
            .node(tfpb::node().name("op").op("BatchToSpaceND").input("input")
            .input("block_shape")
            .input("crops")
            .attr("T", DT_FLOAT)
            );
        let graph = graph.write_to_bytes()?;
        let inputs = vec!(("input", b.clone()), ("block_shape", bs.clone()), ("crops", c.clone()));
        compare(&graph, inputs, "op")?
    }
}

#[test]
fn space_to_batch_1() {
    use ndarray::*;
    let graph = tfpb::graph()
        .node(placeholder_f32("input"))
        .node(placeholder("block_shape", DT_INT32, tensor_shape(&[2])))
        .node(placeholder_i32("paddings"))
        .node(
            tfpb::node()
                .name("op")
                .op("SpaceToBatchND")
                .input("input")
                .input("block_shape")
                .input("paddings")
                .attr("T", DT_FLOAT),
        );
    let graph = graph.write_to_bytes().unwrap();
    let i = arr4(&[[[[1.0f32], [2.0]], [[3.0], [4.0]]]]).into();
    let bs = arr1(&[2, 2]).into();
    let p = arr2(&[[0, 0], [0, 0]]).into();
    let inputs = vec![("input", i), ("block_shape", bs), ("paddings", p)];
    compare(&graph, inputs, "op").unwrap()
}

#[test]
fn batch_to_space_1() {
    use ndarray::*;
    let graph = tfpb::graph()
        .node(placeholder_f32("input"))
        .node(placeholder("block_shape", DT_INT32, tensor_shape(&[2])))
        .node(placeholder_i32("crops"))
        .node(
            tfpb::node()
                .name("op")
                .op("BatchToSpaceND")
                .input("input")
                .input("block_shape")
                .input("crops")
                .attr("T", DT_FLOAT),
        );
    let graph = graph.write_to_bytes().unwrap();
    let i = arr4(&[[[[1.0f32]]], [[[2.0]]], [[[3.0]]], [[[4.0]]]]).into();
    let bs = arr1(&[2, 2]).into();
    let p = arr2(&[[0, 0], [0, 0]]).into();
    let inputs = vec![("input", i), ("block_shape", bs), ("crops", p)];
    compare(&graph, inputs, "op").unwrap()
}
