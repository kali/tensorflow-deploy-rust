use tfpb::types::DataType;
use analyser::TensorFact;
use {Matrix, Result};
use super::{Input, Op, OpRegister};
use std::sync::Arc;

pub fn register_all_ops(reg: &mut OpRegister) {
    reg.insert("Const", Const::build);
}

#[derive(Debug)]
pub struct Const {
    datatype: DataType,
    value: Arc<Matrix>,
}

impl Const {
    pub fn build(node: &::tfpb::node_def::NodeDef) -> Result<Box<Op>> {
        let datatype = node
                .get_attr()
                .get("dtype")
                .unwrap()
                .get_field_type();
        let mat = node.get_attr_tensor("value")?;

        if mat.datatype() != datatype {
            bail!("Const node {:?} doesn't have the expected {:?} type.", mat, datatype);
        }

        Ok(Box::new(Const {
            datatype,
            value: Arc::new(mat),
        }))
    }
}

impl Op for Const {
    /// Evaluates the operation given the input tensors.
    fn eval(&self, _inputs: Vec<Input>) -> Result<Vec<Input>> {
        Ok(vec![self.value.clone().into()])
    }

    /// Infers properties about the output tensors from the input tensors.
    fn infer_forward(&self, _inputs: Vec<&TensorFact>) -> Result<Option<Vec<TensorFact>>> {
        let output = TensorFact {
            datatype: typefact!(self.datatype),
            shape: self.value.shape().into(),
            value: valuefact!(self.value.as_ref().clone())
        };

        Ok(Some(vec![output]))
    }

    /// Infers properties about the input tensors from the output tensors.
    fn infer_backward(&self, _outputs: Vec<&TensorFact>) -> Result<Option<Vec<TensorFact>>> {
        info!("Const operation is a leaf, nothing to infer backwards.");
        Ok(None)
    }
}
