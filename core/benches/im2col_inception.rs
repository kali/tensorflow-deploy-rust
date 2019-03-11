#[macro_use]
extern crate criterion;
extern crate ndarray;
extern crate tract_core;
use criterion::Criterion;

use tract_core::ops::nn::PaddingSpec;
use tract_core::ops::nn::PaddingSpec::Valid;
use tract_core::ops::nn::PaddingSpec::SameUpper as Same;
use tract_core::ops::prelude::*;
use tract_core::tvec;
use tract_core::DatumType;
use tract_core::Tensor;
use tract_core::TensorFact;

fn b(
    c: &mut Criterion,
    name: &str,
    h: usize,
    w: usize,
    ci: usize,
    kh: usize,
    kw: usize,
    co: usize,
    stride: usize,
    padding: PaddingSpec,
) {
    let image = Tensor::from(ndarray::Array4::<f32>::zeros((1, h, w, ci)));
    let kernel = Tensor::from(ndarray::Array4::<f32>::zeros((kh, kw, ci, co)));
    let conv = tract_core::ops::nn::Conv::new(
        tract_core::ops::nn::DataFormat::NHWC,
        tract_core::ops::nn::KernelFormat::HWIO,
        None,
        Some(kernel.shape()[0..2].into()),
        padding,
        Some(tvec!(stride, stride)),
        1,
    );
    let input_fact = TensorFact::dt_shape(DatumType::F32, image.shape());
    let kernel_fact = TensorFact::from(kernel);
    let unary = conv
        .reduce(
            tvec!(&input_fact, &kernel_fact),
            tvec!(),
            ReductionPhase::Normalize,
        )
        .unwrap()
        .unwrap()
        .ops
        .remove(0);
    let im2col = unary
        .reduce(
            tvec!(&input_fact, &kernel_fact),
            tvec!(),
            ReductionPhase::Codegen,
        )
        .unwrap()
        .unwrap()
        .ops
        .remove(0);
    assert_eq!(im2col.name(), "Im2col");
    let args = tvec!(image.into());
    c.bench_function(name, move |b| {
        let ref op = im2col.as_stateless().unwrap();
        b.iter(|| op.eval(args.clone()).unwrap())
    });
}

macro_rules! b {
    ($id:ident, $($args:expr),*) => {
        #[allow(non_snake_case)]
        fn $id(c: &mut Criterion) {
            b(c, stringify!($id), $($args),*);
        }
    }
}

b!(Conv2d_2a_3x3, 149, 149, 32, 3, 3, 32, 1, Valid);
b!(Conv2d_2b_3x3, 147, 147, 32, 3, 3, 64, 1, Same);

criterion_group!(benches,
    Conv2d_2a_3x3,
    Conv2d_2b_3x3,
);
criterion_main!(benches);
