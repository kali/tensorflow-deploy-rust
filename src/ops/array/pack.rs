use std::marker::PhantomData;

use analyser::{TensorFact, ShapeFact};
use analyser::helpers::most_specific_shape;
use analyser::helpers::infer_forward_concrete;
use Result;
use super::{Input, Op};
use tensor::Datum;

#[derive(Debug, Default, new)]
pub struct Pack<T: Datum> {
    n: usize, // The number of inputs
    axis: usize,
    _phantom: PhantomData<T>,
}

pub fn pack(pb: &::tfpb::node_def::NodeDef) -> Result<Box<Op>> {
    let dtype = pb.get_attr_datatype("T")?;
    let n = pb.get_input().len();
    let axis = pb.get_attr_int("axis")?;

    Ok(boxed_new!(Pack(dtype)(n, axis)))
}

impl<T> Op for Pack<T>
where
    T: Datum,
{
    /// Evaluates the operation given the input tensors.
    fn eval(&self, inputs: Vec<Input>) -> Result<Vec<Input>> {
        use ndarray::Axis;
        let views = inputs
            .iter()
            .map(|m| {
                Ok(T::mat_to_view(&*m)?.insert_axis(Axis(self.axis)))
            })
            .collect::<Result<Vec<_>>>()?;
        let array = ::ndarray::stack(Axis(self.axis), &*views)?;
        Ok(vec![T::array_into_tensor(array).into()])
    }

    /// Infers properties about the output tensors from the input tensors.
    fn infer_forward(&self, inputs: Vec<&TensorFact>) -> Result<Option<Vec<TensorFact>>> {
        if inputs.len() < 1 {
            bail!("Pack operation needs at least one input.");
        }

        if let Some(output) = infer_forward_concrete(self, &inputs)? {
            return Ok(Some(output));
        }

        // If we don't know the actual value, we can still compute the shape.
        let n = inputs.len();
        let shapes = inputs
            .iter()
            .map(|t| &t.shape);

        // We get the most specific shape, and replace the axis with an unknown.
        let shape = match most_specific_shape(shapes)? {
            Some(s) => {
                let mut dims = s.dims.clone();
                dims.insert(self.axis, dimfact!(n));
                ShapeFact::closed(dims)
            },

            None => shapefact![..]
        };

        let output = TensorFact {
            datatype: inputs[0].datatype,
            shape,
            value: valuefact!(_),
        };

        Ok(Some(vec![output]))
    }

    /// Infers properties about the input tensors from the output tensors.
    fn infer_backward(&self, outputs: Vec<&TensorFact>) -> Result<Option<Vec<TensorFact>>> {
        if outputs.len() < 1 {
            bail!("Pack operation only supports one output.");
        }

        // The operation adds a dimension, so all we have to do is remove it.
        let mut inner = outputs[0].shape.dims.clone();
        let shape = if outputs[0].shape.open {
            if self.axis > inner.len() {
                inner.remove(self.axis);
            }

            ShapeFact::open(inner)
        } else {
            inner.remove(self.axis);
            ShapeFact::closed(inner)
        };

        let input = TensorFact {
            datatype: outputs[0].datatype,
            shape,
            value: valuefact!(_)
        };

        Ok(Some(vec![input; self.n]))
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use Tensor;
    use super::*;
    use ndarray::arr2;

    #[test]
    fn pack_0() {
        let inputs = vec![
            Tensor::i32s(&[2], &[1, 4]).unwrap().into(),
            Tensor::i32s(&[2], &[2, 5]).unwrap().into(),
            Tensor::i32s(&[2], &[3, 6]).unwrap().into(),
        ];
        assert_eq!(
            Pack::<i32>::new(3, 0)
                .eval(inputs.clone())
                .unwrap()
                .remove(0)
                .into_tensor(),
            Tensor::from(arr2(&[[1, 4], [2, 5], [3, 6]]))
        );
        assert_eq!(
            Pack::<i32>::new(3, 1)
                .eval(inputs.clone())
                .unwrap()
                .remove(0)
                .into_tensor(),
            Tensor::from(arr2(&[[1, 2, 3], [4, 5, 6]]))
        );
    }

    #[test]
    fn pack_1() {
        let pack = Pack::<i32>::new(3, 0);
        let input = Tensor::i32s(&[0], &[]).unwrap();
        let exp: Tensor = Tensor::i32s(&[1, 0], &[]).unwrap();
        let found = pack.eval(vec![input.into()]).unwrap();

        assert!(
            exp.close_enough(&found[0]),
            "expected: {:?} found: {:?}",
            exp,
            found[0]
        )
    }
}
