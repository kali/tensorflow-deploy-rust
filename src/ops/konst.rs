use ops::prelude::*;
use analyser::rules::prelude::*;

#[derive(Debug, Clone, new)]
pub struct Const {
    value: Value,
}

impl Const {
    pub fn for_tensor(tensor: Tensor) -> Const {
        let value: Value = tensor.into();
        Const {
            value: value.into_shared(),
        }
    }
}

impl Op for Const {
    /// Evaluates the operation given the input tensors.
    fn eval(&self, _inputs: TVec<Value>) -> Result<TVec<Value>> {
        Ok(tvec![self.value.clone()])
    }

    fn const_value(&self) -> Option<Value> {
        Some(self.value.clone())
    }
}

impl InferenceRulesOp for Const {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        solver: &mut Solver<'r>,
        inputs: &'p TensorsProxy,
        outputs: &'p TensorsProxy,
    ) {
        solver.equals(&inputs.len, 0).equals(&outputs.len, 1);
    }
}
