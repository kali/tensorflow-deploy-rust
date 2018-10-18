use analyser::rules::prelude::*;
use ndarray::prelude::*;
use ops::prelude::*;

use num::cast::AsPrimitive;
use num::Float;

#[derive(Debug, Clone, new)]
pub struct Gemm {
    alpha: f32,
    beta: f32,
    trans_a: bool,
    trans_b: bool,
}

impl Gemm {
    fn eval_t<T: Datum + Float>(&self, mut inputs: TVec<Value>) -> TfdResult<TVec<Value>>
    where
        f32: AsPrimitive<T>,
    {
        let (a, b, c) = args_3!(inputs);
        let a = a.to_array_view::<T>()?.into_dimensionality()?;
        let at = if self.trans_a { a.t() } else { a };
        let b = b.to_array_view::<T>()?.into_dimensionality()?;
        let bt = if self.trans_b { b.t() } else { b };
        let c_shape = (at.rows(), bt.cols());
        let mut c = if c.shape() == &[c_shape.0, c_shape.1] {
            c.into_array::<T>()?
                .into_dimensionality::<Ix2>()?
                .to_owned()
        } else {
            c.to_array_view::<T>()?
                .broadcast(c_shape)
                .ok_or_else(|| format!("Incompatible broadcast: {:?} to {:?}", c.shape(), c_shape))?
                .to_owned()
        };
        ::ndarray::linalg::general_mat_mul(self.alpha.as_(), &at, &bt, self.beta.as_(), &mut c);
        Ok(tvec!(c.into()))
    }
}

impl Op for Gemm {
    fn name(&self) -> &str {
        "Gemm"
    }
}

impl StatelessOp for Gemm {
    fn eval(&self, inputs: TVec<Value>) -> TfdResult<TVec<Value>> {
        dispatch_floatlike!(Self::eval_t(inputs[0].datum_type())(self, inputs))
    }
}

impl InferenceRulesOp for Gemm {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) -> InferenceResult {
        s.equals(&inputs.len, 3)?;
        s.equals(&inputs[0].rank, 2)?;
        s.equals(&inputs[1].rank, 2)?;
        s.equals(&outputs.len, 1)?;
        s.equals(&outputs[0].rank, 2)?;
        s.equals(&inputs[0].datum_type, &outputs[0].datum_type)?;
        s.equals(&inputs[1].datum_type, &outputs[0].datum_type)?;
        s.equals(&inputs[2].datum_type, &outputs[0].datum_type)?;
        let (ca, ra) = if self.trans_a { (0, 1) } else { (1, 0) };
        let (cb, rb) = if self.trans_b { (0, 1) } else { (1, 0) };
        s.equals(&inputs[0].shape[ra], &outputs[0].shape[0])?;
        s.equals(&inputs[0].shape[ca], &inputs[1].shape[rb])?;
        s.equals(&inputs[1].shape[cb], &outputs[0].shape[1])?;
        Ok(())
    }
}

#[derive(Debug, Clone, new)]
pub struct GemmUnaryA {
    alpha: f32,
    beta: f32,
    trans_a: bool,
    trans_b: bool,
    b: Tensor,
    c: Tensor,
}

impl GemmUnaryA {
    fn eval_t<T: Datum + Float>(&self, mut inputs: TVec<Value>) -> TfdResult<TVec<Value>>
    where
        f32: AsPrimitive<T>,
    {
        let a = args_1!(inputs);
        let a = a.to_array_view::<T>()?.into_dimensionality()?;
        let at = if self.trans_a { a.t() } else { a };
        let b = self.b.to_array_view::<T>()?.into_dimensionality()?;
        let bt = if self.trans_b { b.t() } else { b };
        let mut c = self
            .c
            .to_array_view::<T>()?
            .into_dimensionality()?
            .to_owned();
        ::ndarray::linalg::general_mat_mul(self.alpha.as_(), &at, &bt, self.beta.as_(), &mut c);
        Ok(tvec!(c.into()))
    }
}

impl Op for GemmUnaryA {
    fn name(&self) -> &str {
        "GemmUnaryA"
    }
}

impl StatelessOp for GemmUnaryA {
    fn eval(&self, inputs: TVec<Value>) -> TfdResult<TVec<Value>> {
        dispatch_floatlike!(Self::eval_t(inputs[0].datum_type())(self, inputs))
    }
}

impl InferenceRulesOp for GemmUnaryA {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) -> InferenceResult {
        s.equals(&inputs.len, 1)?;
        s.equals(&inputs[0].rank, 2)?;
        s.equals(&outputs.len, 1)?;
        s.equals(&outputs[0].rank, 2)?;
        s.equals(&inputs[0].datum_type, &outputs[0].datum_type)?;
        s.equals(&inputs[1].datum_type, &outputs[0].datum_type)?;
        s.equals(&inputs[2].datum_type, &outputs[0].datum_type)?;
        let (ca, ra) = if self.trans_a { (0, 1) } else { (1, 0) };
        let (cb, rb) = if self.trans_b { (0, 1) } else { (1, 0) };
        s.equals(&inputs[0].shape[ra], &outputs[0].shape[0])?;
        s.equals(&inputs[0].shape[ca], self.b.shape()[rb].to_dim())?;
        s.equals(self.b.shape()[cb].to_dim(), &outputs[0].shape[1])?;
        Ok(())
    }
}

#[derive(Debug, Clone, new)]
pub struct GemmUnaryB {
    alpha: f32,
    beta: f32,
    trans_a: bool,
    trans_b: bool,
    a: Tensor,
    c: Tensor,
}

impl GemmUnaryB {
    fn eval_t<T: Datum + Float>(&self, mut inputs: TVec<Value>) -> TfdResult<TVec<Value>>
    where
        f32: AsPrimitive<T>,
    {
        let b = args_1!(inputs);
        let b = b.to_array_view::<T>()?.into_dimensionality()?;
        let a = self.a.to_array_view::<T>()?.into_dimensionality()?;
        let at = if self.trans_a { a.t() } else { a };
        let bt = if self.trans_b { b.t() } else { b };
        let mut c = self
            .c
            .to_array_view::<T>()?
            .into_dimensionality()?
            .to_owned();
        ::ndarray::linalg::general_mat_mul(self.alpha.as_(), &at, &bt, self.beta.as_(), &mut c);
        Ok(tvec!(c.into()))
    }
}

impl Op for GemmUnaryB {
    fn name(&self) -> &str {
        "GemmUnaryB"
    }
}

impl StatelessOp for GemmUnaryB {
    fn eval(&self, inputs: TVec<Value>) -> TfdResult<TVec<Value>> {
        dispatch_floatlike!(Self::eval_t(inputs[0].datum_type())(self, inputs))
    }
}

impl InferenceRulesOp for GemmUnaryB {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) -> InferenceResult {
        s.equals(&inputs.len, 1)?;
        s.equals(&inputs[0].rank, 2)?;
        s.equals(&outputs.len, 1)?;
        s.equals(&outputs[0].rank, 2)?;
        s.equals(&inputs[0].datum_type, &outputs[0].datum_type)?;
        s.equals(&inputs[1].datum_type, &outputs[0].datum_type)?;
        s.equals(&inputs[2].datum_type, &outputs[0].datum_type)?;
        let (ca, ra) = if self.trans_a { (0, 1) } else { (1, 0) };
        let (cb, rb) = if self.trans_b { (0, 1) } else { (1, 0) };
        s.equals(self.a.shape()[ra].to_dim(), &outputs[0].shape[0])?;
        s.equals(self.a.shape()[ca].to_dim(), &inputs[0].shape[rb])?;
        s.equals(&inputs[0].shape[cb], &outputs[0].shape[1])?;
        Ok(())
    }
}
