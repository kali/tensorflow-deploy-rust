use analyser::rules::prelude::*;
use ndarray::prelude::*;
use ops::prelude::*;

mod concatv2;
mod fill;
mod pack;
mod pad;
mod reshape;
mod squeeze;
mod strided_slice;

pub fn register_all_ops(reg: &mut OpRegister) {
    reg.insert("ConcatV2", concatv2::build);
    reg.insert("ExpandDims", ExpandDims::build);
    reg.insert("Identity", Identity::build);
    reg.insert("Fill", fill::fill);
    reg.insert("Pack", pack::pack);
    reg.insert("Pad", pad::pad);
    reg.insert("Placeholder", Placeholder::build);
    reg.insert("Reshape", reshape::reshape);
    reg.insert("Shape", Shape::build);
    reg.insert("Squeeze", squeeze::squeeze);
    reg.insert("StridedSlice", strided_slice::build);
}

#[derive(Debug, Clone)]
pub struct ExpandDims;

impl ExpandDims {
    pub fn build(_pb: &::tfpb::node_def::NodeDef) -> Result<Box<Op>> {
        Ok(Box::new(ExpandDims))
    }
}

impl Op for ExpandDims {
    /// Returns the attributes of the operation and their values.
    fn get_attributes(&self) -> HashMap<&'static str, Attr> {
        hashmap!{}
    }

    /// Evaluates the operation given the input tensors.
    fn eval(&self, mut inputs: TVec<Value>) -> Result<TVec<Value>> {
        let (data, dims) = args_2!(inputs);
        let data = data.into_tensor()
            .take_f32s()
            .ok_or("Expected a f32 matrix")?;
        let dims = dims.as_i32s().ok_or("Expected a i32 matrix")?;
        let mut shape = data.shape().to_vec();
        for d in dims.iter() {
            if *d >= 0 {
                shape.insert(*d as usize, 1);
            } else {
                Err(format!("unimplemented ExpandDims with negative parameter"))?
            }
        }
        Ok(tvec![Tensor::from(data.into_shape(shape)?).into()])
    }

    /// Evaluates one step of the operation on the given input tensors.
    fn step(
        &self,
        mut inputs: TVec<StepValue>,
        _: &mut Box<OpBuffer>,
    ) -> Result<Option<TVec<Value>>> {
        let (data, dims) = args_2!(inputs);

        let dims = if let StepValue::Const(dims) = dims {
            dims
        } else {
            bail!("Dims input should not be streamed.")
        };

        let data = data.into_stream().ok_or("Data input should be streamed.")?;

        match data.chunk {
            None => Ok(None),
            Some(tv) => Ok(Some(self.eval(tvec![tv, dims])?)),
        }
    }
}

impl InferenceRulesOp for ExpandDims {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        solver: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) {
        let data = &inputs[0];
        let dims = &inputs[1];
        let output = &outputs[0];

        solver
            .equals(&inputs.len, 2)
            .equals(&outputs.len, 1)
            .equals(&dims.datum_type, DatumType::I32)
            .equals(&dims.rank, 0)
            .equals(&data.datum_type, &output.datum_type)
            .equals_zero(data.rank.bex() + 1 - &output.rank)
            .given(&dims.value, move |solver, index: Tensor| {
                let index = index.as_i32().unwrap() as usize; // enforced

                for i in 0..index {
                    solver.equals(&output.shape[i], &data.shape[i]);
                }

                solver.equals(output.shape[index].bex(), 1i32.to_dim().bex());

                solver.given(&data.rank, move |solver, rank| {
                    for i in index..(rank as usize) {
                        solver.equals(&output.shape[i + 1], &data.shape[i]);
                    }
                });
            });
    }
}

#[derive(Debug, Clone)]
pub struct Identity;

impl Identity {
    pub fn build(_: &::tfpb::node_def::NodeDef) -> Result<Box<Op>> {
        Ok(Box::new(Identity))
    }
}

impl Op for Identity {
    /// Returns the attributes of the operation and their values.
    fn get_attributes(&self) -> HashMap<&'static str, Attr> {
        hashmap!{}
    }

    /// Evaluates the operation given the input tensors.
    fn eval(&self, inputs: TVec<Value>) -> Result<TVec<Value>> {
        Ok(inputs)
    }

    /// Evaluates one step of the operation on the given input tensors.
    fn step(
        &self,
        mut inputs: TVec<StepValue>,
        _: &mut Box<OpBuffer>,
    ) -> Result<Option<TVec<Value>>> {
        let input = args_1!(inputs);
        match input.into_value() {
            None => Ok(None),
            Some(tv) => Ok(Some(self.eval(tvec![tv])?)),
        }
    }
}

impl InferenceRulesOp for Identity {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        solver: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) {
        solver
            .equals(&inputs.len, 1)
            .equals(&outputs.len, 1)
            .equals(&inputs[0].datum_type, &outputs[0].datum_type)
            .equals(&inputs[0].shape, &outputs[0].shape);
    }
}

#[derive(Debug, Clone)]
pub struct Placeholder {
    dtype: DatumType,
}

impl Placeholder {
    pub fn build(node: &::tfpb::node_def::NodeDef) -> Result<Box<Op>> {
        Ok(Box::new(Placeholder {
            dtype: node.get_attr_datum_type("dtype")?,
        }))
    }
}

impl Op for Placeholder {
    /// Evaluates the operation given the input tensors.
    fn eval(&self, _inputs: TVec<Value>) -> Result<TVec<Value>> {
        panic!("Placeholder should not get evaluated")
    }

    /// Returns the attributes of the operation and their values.
    fn get_attributes(&self) -> HashMap<&'static str, Attr> {
        hashmap!{
            "dtype" => Attr::DatumType(self.dtype)
        }
    }

    fn infer_and_propagate(
        &self,
        inputs: TVec<TensorFact>,
        outputs: TVec<TensorFact>,
    ) -> Result<(TVec<TensorFact>, TVec<TensorFact>)> {
        use ops::InferenceOp;
        self.infer(inputs, outputs)
    }
}

impl InferenceRulesOp for Placeholder {
    /// Registers the inference rules of the operator.
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        solver: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) {
        solver
            .equals(&inputs.len, 0)
            .equals(&outputs.len, 1)
            .equals(&outputs[0].datum_type, self.dtype);
    }
}

#[derive(Debug, Clone)]
pub struct Shape;

impl Shape {
    pub fn build(_pb: &::tfpb::node_def::NodeDef) -> Result<Box<Op>> {
        Ok(Box::new(Shape))
    }
}

impl Op for Shape {
    /// Returns the attributes of the operation and their values.
    fn get_attributes(&self) -> HashMap<&'static str, Attr> {
        hashmap!{}
    }

    /// Evaluates the operation given the input tensors.
    fn eval(&self, inputs: TVec<Value>) -> Result<TVec<Value>> {
        let data = inputs[0].as_f32s().ok_or("Expect input #0 to be f32")?;
        let shape: Vec<i32> = data.shape().into_iter().map(|s| *s as i32).collect();
        Ok(tvec![Tensor::from(Array1::from_vec(shape)).into()])
    }
}

impl InferenceRulesOp for Shape {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        solver: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) {
        solver
            .equals(&inputs.len, 1)
            .equals(&outputs.len, 1)
            .equals(&outputs[0].datum_type, DatumType::TDim)
            .equals(&outputs[0].rank, 1)
            .given(&inputs[0].rank, move |solver, r| {
                solver.equals(&outputs[0].shape[0], r.to_dim());
            })
            .given(&outputs[0].shape[0], move |solver, r| {
                if let Ok(d) = r.to_integer() {
                    solver.equals(&inputs[0].rank, d);
                }
            })
            .given(&inputs[0].shape, move |solver, shape: Vec<TDim>| {
                let array1: Array1<TDim> = Array1::from_vec(shape);
                let tensor: Tensor = Tensor::from(array1);
                solver.equals(&outputs[0].value, tensor);
            })
            .given(&outputs[0].value, move |solver, shape: Tensor| {
                let shape = shape.take_dims().unwrap(); // checked
                solver.equals(
                    &inputs[0].shape,
                    shape.into_iter().cloned().collect::<Vec<TDim>>(),
                );
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tfpb::node;

    #[test]
    fn shape_inference_1() {
        let input = TensorFact {
            datum_type: typefact!(DatumType::F32),
            shape: shapefact![1, _, _; ..],
            value: valuefact!(_),
        };

        let output = TensorFact {
            datum_type: typefact!(DatumType::TDim),
            shape: shapefact![_],
            value: valuefact!(_),
        };

        assert_forward!(Shape::build(&node()).unwrap(), input, output);
    }

    #[test]
    fn shape_inference_2() {
        let input = TensorFact {
            datum_type: typefact!(DatumType::F32),
            shape: shapefact![1, _, _],
            value: valuefact!(_),
        };

        let output = TensorFact {
            datum_type: typefact!(DatumType::TDim),
            shape: shapefact![3],
            value: valuefact!(_),
        };

        assert_forward!(Shape::build(&node()).unwrap(), input, output);
    }

    #[test]
    fn shape_inference_3() {
        let input = TensorFact {
            datum_type: typefact!(DatumType::F32),
            shape: shapefact![1, 2, 3],
            value: valuefact!(_),
        };

        let output = TensorFact {
            datum_type: typefact!(DatumType::TDim),
            shape: shapefact![3],
            value: valuefact!(Tensor::dims(&[3], &[1.to_dim(), 2.to_dim(), 3.to_dim()]).unwrap()),
        };

        assert_forward!(Shape::build(&node()).unwrap(), input, output);
    }

    #[test]
    fn shape_inference_4() {
        let input = TensorFact {
            datum_type: typefact!(_),
            shape: shapefact![1, 2, 3],
            value: valuefact!(_),
        };

        let output = TensorFact {
            datum_type: typefact!(DatumType::TDim),
            shape: shapefact![3],
            value: valuefact!(Tensor::dims(&[3], &[1.to_dim(), 2.to_dim(), 3.to_dim()]).unwrap()),
        };

        assert_backward!(Shape::build(&node()).unwrap(), input, output);
    }
}
