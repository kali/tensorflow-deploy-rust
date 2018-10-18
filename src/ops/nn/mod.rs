mod avgpool;
mod batch_norm;
mod conv;
mod data_formats;
mod global_pools;
mod layer_max;
mod lrn;
mod maxpool;
mod padding;
mod patches;
mod reduce;

pub use self::avgpool::AvgPool;
pub use self::batch_norm::BatchNorm;
pub use self::conv::{Conv, ConvUnary, FixedParamsConv};
pub use self::data_formats::{DataFormat, DataShape};
pub use self::global_pools::{GlobalAvgPool, GlobalLpPool, GlobalMaxPool};
pub use self::layer_max::{LayerHardmax, LayerLogSoftmax, LayerSoftmax};
pub use self::lrn::Lrn;
pub use self::maxpool::MaxPool;
pub use self::padding::PaddingSpec;
pub use self::patches::Patch;
pub use self::reduce::{Reduce, Reducer};

use num::traits::AsPrimitive;

element_map!(Relu, [f32, i32], |x| if x < 0 as _ { 0 as _ } else { x });
element_map!(Sigmoid, [f32], |x| ((-x).exp() + 1.0).recip());
element_map!(Softplus, [f32], |x| (x.exp() + 1.0).ln());
element_map!(Softsign, [f32], |x| x / (x.abs() + 1.0));

element_map_with_params!(
    Elu,
    [f32, f64],
    { alpha: f32 },
    fn eval_one<T>(elu: &Elu, x: T) -> T
    where
        T: Datum + ::num::Float,
        f32: ::num::cast::AsPrimitive<T>,
    {
        if x < 0.0.as_() {
            elu.alpha.as_() * (x.exp() - 1.0.as_())
        } else {
            x
        }
    }
);

element_map_with_params!(Hardsigmoid, [f32, f64], {alpha: f32, beta: f32},
    fn eval_one<T>(hs: &Hardsigmoid, x:T) -> T
    where T: Datum+::num::Float, f32: ::num::cast::AsPrimitive<T>
    {
        (hs.alpha.as_() * x + hs.beta.as_()).min(1.0.as_()).max(0.0.as_())
    }
);

element_map_with_params!(
    LeakyRelu,
    [f32, f64],
    { alpha: f32 },
    fn eval_one<T>(lr: &LeakyRelu, x: T) -> T
    where
        T: Datum + ::num::Float,
        f32: ::num::cast::AsPrimitive<T>,
    {
        if x < 0.0.as_() {
            lr.alpha.as_() * x
        } else {
            x
        }
    }
);

element_map_with_params!(Selu, [f32, f64], {alpha: f32, gamma: f32},
    fn eval_one<T>(s: &Selu, x:T) -> T
    where T: Datum+::num::Float, f32: ::num::cast::AsPrimitive<T>
    {
        if x < 0.0.as_() {
            s.gamma.as_() * (s.alpha.as_() * x.exp() - s.alpha.as_())
        } else {
            s.gamma.as_() * x
        }
    }
);
