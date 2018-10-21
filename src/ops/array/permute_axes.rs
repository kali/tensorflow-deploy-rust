use ops::prelude::*;

#[derive(Debug, Clone, new)]
pub struct PermuteAxes {
    pub axes: Option<Vec<usize>>,
}

impl PermuteAxes {
    fn compute_shape<D: DimLike>(&self, input: &[D]) -> Vec<D> {
        if let Some(ref axes) = self.axes {
            let mut new_shape = vec!(D::zero(); input.len());
            for (ix, &d) in axes.iter().enumerate() {
                new_shape[ix] = input[d];
            }
            new_shape
        } else {
            let mut new_shape = input.to_vec();
            new_shape.reverse();
            new_shape
        }
    }

    /// Evaluates the operation given the input tensors.
    fn eval_t<T: Datum>(&self, input: Value) -> TfdResult<TVec<Value>> {
        if let Some(ref axes) = self.axes {
            Ok(tvec![input.into_array::<T>()?.permuted_axes(&**axes).into()])
        } else {
            Ok(tvec![input.into_array::<T>()?.reversed_axes().into()])
        }
    }
}

impl Op for PermuteAxes {
    fn name(&self) -> &str {
        "PermuteAxes"
    }
}

impl StatelessOp for PermuteAxes {
    fn eval(&self, mut inputs: TVec<Value>) -> TfdResult<TVec<Value>> {
        let input = args_1!(inputs);
        dispatch_datum!(Self::eval_t(input.datum_type())(self, input))
    }
}

impl InferenceRulesOp for PermuteAxes {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) -> InferenceResult {
        s.equals(&outputs.len, 1)?;
        s.equals(&outputs[0].datum_type, &inputs[0].datum_type)?;
        s.equals(&outputs[0].rank, &inputs[0].rank)?;
        s.given(&inputs[0].shape, move |s, shape| {
            let output_shape = self.compute_shape(&shape);
            s.equals(&outputs[0].shape, output_shape)
        })
    }
}
