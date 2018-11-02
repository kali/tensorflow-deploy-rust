#![allow(non_snake_case)]
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;
extern crate ndarray;
#[macro_use]
extern crate proptest;
extern crate protobuf;
extern crate simplelog;
extern crate tensorflow;
#[macro_use]
extern crate tract_core;
extern crate tract_tensorflow;

mod conform;

use conform::*;
use ndarray::prelude::*;
use proptest::prelude::*;
use tract_core::Tensor;
use tract_tensorflow::tfpb;
use tract_tensorflow::tfpb::types::DataType::DT_FLOAT;

fn convolution_pb(v_stride: usize, h_stride: usize, valid: bool) -> ::Result<Vec<u8>> {
    let conv = tfpb::node()
        .name("conv")
        .op("Conv2D")
        .input("data")
        .input("kernel")
        .attr("strides", vec![1, v_stride as i64, h_stride as i64, 1])
        .attr("padding", if valid { "VALID" } else { "SAME" })
        .attr("T", DT_FLOAT);

    let graph = tfpb::graph()
        .node(placeholder_f32("data"))
        .node(placeholder_f32("kernel"))
        .node(conv);

    Ok(graph.write_to_bytes()?)
}

fn img_and_ker() -> BoxedStrategy<(Tensor, Tensor, (usize, usize))> {
    (1usize..8, 1usize..8, 1usize..8, 1usize..8)
        .prop_flat_map(|(ic, kh, kw, kc)| (1usize..10, kh..33, kw..33, Just((ic, kh, kw, kc))))
        .prop_flat_map(|(ib, ih, iw, (ic, kh, kw, kc))| {
            let i_size = ib * iw * ih * ic;
            let k_size = kw * kh * kc * ic;
            (
                Just((ib, ih, iw, ic)),
                Just((kh, kw, ic, kc)),
                ::proptest::collection::vec(-9i32..9, i_size..i_size + 1),
                ::proptest::collection::vec(-9i32..9, k_size..k_size + 1),
                (1..(kh + 1), 1..(kw + 1)),
            )
        }).prop_map(|(img_shape, ker_shape, img, ker, strides)| {
            (
                Array::from_vec(img.into_iter().map(|i| i as f32).collect())
                    .into_shape(img_shape)
                    .unwrap()
                    .into(),
                Array::from_vec(ker.into_iter().map(|i| i as f32).collect())
                    .into_shape(ker_shape)
                    .unwrap()
                    .into(),
                strides,
            )
        }).boxed()
}

proptest! {
    #[test]
    fn conv_compare((ref i, ref k, ref strides) in img_and_ker(),
                       valid in ::proptest::bool::ANY) {
//        ::conform::setup_test_logger();
        if valid {
            prop_assume!(i.shape()[1] >= k.shape()[0]);
            prop_assume!(i.shape()[2] >= k.shape()[1]);
        }
        let model = convolution_pb(strides.0, strides.1, valid).unwrap();
        compare(&model, vec!(("data", i.clone()), ("kernel", k.clone())), "conv")?;
    }
}

proptest! {
    #[test]
    fn conv_infer_facts((ref i, ref k, ref strides) in img_and_ker(),
                       valid in ::proptest::bool::ANY) {
//        ::conform::setup_test_logger();
        if valid {
            prop_assume!(i.shape()[1] >= k.shape()[0]);
            prop_assume!(i.shape()[2] >= k.shape()[1]);
        }
        let model = convolution_pb(strides.0, strides.1, valid).unwrap();
        infer(&model, vec!(("data", i.clone()), ("kernel", k.clone())), "conv")?;
    }
}

#[test]
fn conv_infer_facts_1() {
    //   ::conform::setup_test_logger();
    let i: Tensor = ArrayD::<f32>::zeros(vec![1, 2, 2, 2]).into();
    let k: Tensor = ArrayD::<f32>::zeros(vec![2, 2, 2, 1]).into();
    let model = convolution_pb(1, 1, false).unwrap();
    infer(
        &model,
        vec![("data", i.into()), ("kernel", k.into())],
        "conv",
    ).unwrap();
}

#[test]
fn conv_eval_1() {
    use tract_core::tensor::arr4;
    //   ::conform::setup_test_logger();
    let i: Tensor = Tensor::from(arr4(&[[[[0.0f32, 0.0], [1.0, 0.0]]]]));
    let k: Tensor = Tensor::from(arr4(&[[[[0.0f32], [0.0]], [[1.0], [0.0]]]]));
    let model = convolution_pb(1, 1, false).unwrap();
    compare(
        &model,
        vec![("data", i.into()), ("kernel", k.into())],
        "conv",
    ).unwrap();
}
