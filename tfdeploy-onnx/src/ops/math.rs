use tfdeploy::ops as tfdops;

use ops::OpRegister;

pub fn register_all_ops(reg: &mut OpRegister) {
    reg.insert("Add", |_| Ok(Box::new(tfdops::math::Add::default())));
    reg.insert("Sub", |_| Ok(Box::new(tfdops::math::Sub::default())));
    reg.insert("Mul", |_| Ok(Box::new(tfdops::math::Mul::default())));
    reg.insert("Div", |_| Ok(Box::new(tfdops::math::Div::default())));

    reg.insert("Sum", |_| Ok(Box::new(tfdops::math::AddN::default())));
    reg.insert("Max", |_| Ok(Box::new(tfdops::math::MaxN::default())));
    reg.insert("Min", |_| Ok(Box::new(tfdops::math::MinN::default())));
    reg.insert("Mean", |_| Ok(Box::new(tfdops::math::MeanN::default())));

    reg.insert("Abs", |_| Ok(Box::new(tfdops::math::Abs::default())));
    reg.insert("Ceil", |_| Ok(Box::new(tfdops::math::Ceil::default())));
    reg.insert("Floor", |_| Ok(Box::new(tfdops::math::Floor::default())));

    reg.insert("Cos", |_| Ok(Box::new(tfdops::math::Cos::default())));
    reg.insert("Sin", |_| Ok(Box::new(tfdops::math::Sin::default())));
    reg.insert("Tan", |_| Ok(Box::new(tfdops::math::Tan::default())));
    reg.insert("Acos", |_| Ok(Box::new(tfdops::math::Acos::default())));
    reg.insert("Asin", |_| Ok(Box::new(tfdops::math::Asin::default())));
    reg.insert("Atan", |_| Ok(Box::new(tfdops::math::Atan::default())));

    reg.insert("Exp", |_| Ok(Box::new(tfdops::math::Exp::default())));
    reg.insert("Ln", |_| Ok(Box::new(tfdops::math::Ln::default())));
    reg.insert("Sqrt", |_| Ok(Box::new(tfdops::math::Sqrt::default())));
    reg.insert("Rsqrt", |_| Ok(Box::new(tfdops::math::Rsqrt::default())));

    reg.insert("Neg", |_| Ok(Box::new(tfdops::math::Neg::default())));
    reg.insert("Recip", |_| Ok(Box::new(tfdops::math::Recip::default())));

    reg.insert("Pow", |_| Ok(Box::new(tfdops::math::Pow::default())));

    reg.insert("Tanh", |_| Ok(Box::new(tfdops::math::Tanh::default())));
}

