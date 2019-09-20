use crate::internal::*;
use ndarray::*;

#[derive(Debug, Clone, new, Default)]
pub struct Split {
    axis: usize,
    outputs: usize,
    split: Option<Vec<usize>>,
}

impl Split {
    fn split_dims<D: DimLike>(&self, input: D) -> TractResult<TVec<D>> {
        if let Some(ref split) = self.split.as_ref() {
            Ok(split.iter().map(|&d| D::from(d)).collect())
        } else {
            Ok(tvec!(input/self.outputs;self. outputs))
        }
    }
    fn eval_t<T: Datum>(&self, input: Arc<Tensor>) -> TractResult<TVec<Arc<Tensor>>> {
        let mut current = 0;
        let input = input.to_array_view::<T>()?;
        Ok(self
            .split_dims(input.shape()[self.axis])?
            .iter()
            .map(|&d| {
                let slice = if d > 0 {
                    input.slice_axis(Axis(self.axis), (current..current + d).into()).to_owned()
                } else {
                    let mut shape: TVec<usize> = input.shape().into();
                    shape[self.axis] = 0;
                    ArrayD::<T>::default(&*shape)
                };
                current += d;
                slice.into_arc_tensor()
            })
            .collect())
    }
}

impl Op for Split {
    fn name(&self) -> Cow<str> {
        "Split".into()
    }

    op_as_typed_op!();
    not_a_pulsed_op!();
}

impl StatelessOp for Split {
    /// Evaluates the operation given the input tensors.
    fn eval(&self, mut inputs: TVec<Arc<Tensor>>) -> TractResult<TVec<Arc<Tensor>>> {
        let input = args_1!(inputs);
        dispatch_datum!(Self::eval_t(input.datum_type())(self, input))
    }
}

impl InferenceRulesOp for Split {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p [TensorProxy],
        outputs: &'p [TensorProxy],
    ) -> InferenceResult {
        check_input_arity(&inputs, 1)?;
        check_output_arity(&outputs, self.outputs)?;
        (0..self.outputs).try_for_each(|i| {
            s.equals(&inputs[0].datum_type, &outputs[i].datum_type)?;
            s.equals(&inputs[0].rank, &outputs[i].rank)
        })?;
        s.given(&inputs[0].shape, move |s, shape| {
            let dims = self.split_dims(shape[self.axis].clone())?;
            for i in 0..self.outputs {
                let mut shape = shape.clone();
                shape[self.axis] = dims[i].clone();
                s.equals(&outputs[i].shape, shape)?;
            }
            Ok(())
        })?;
        Ok(())
    }

    fn nboutputs(&self) -> TractResult<usize> {
        Ok(self.outputs)
    }

    inference_op_as_op!();
    to_typed!();
}

impl TypedOp for Split {
    typed_op_as_op!();

    fn output_facts(&self, inputs: &[&TypedTensorInfo]) -> TractResult<TVec<TypedTensorInfo>> {
        self.split_dims(inputs[0].shape.dim(self.axis))?
            .into_iter()
            .map(|d| {
                let mut fact = inputs[0].clone();
                fact.shape.set_dim(self.axis, d)?;
                Ok(fact)
            })
            .collect()
    }
}
