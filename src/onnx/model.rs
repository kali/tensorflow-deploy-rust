use std::collections::HashMap;
use std::sync::Arc;
use std::{fs, path};

use model::{Model, Node, OutletId, RawModel};
use Result;

use analyser::prelude::*;

use onnx::pb;
use onnx::Protobuf;

/// Load a ONNX protobul model from a file.
pub fn for_path<P: AsRef<path::Path>>(p: P) -> Result<Model> {
    for_reader(fs::File::open(p)?)
}

/// Load a ONNX model from a reader.
pub fn for_reader<R: ::std::io::Read>(r: R) -> Result<Model> {
    from_onnx(model_proto_for_reader(r)?)
}

/// Load a ONNX protobuf graph def from a path
pub fn model_proto_for_path<P: AsRef<path::Path>>(p: P) -> Result<pb::ModelProto> {
    model_proto_for_reader(fs::File::open(p)?)
}

/// Load a ONNX protobuf graph def from a reader.
pub fn model_proto_for_reader<R: ::std::io::Read>(mut r: R) -> Result<pb::ModelProto> {
    Ok(::protobuf::parse_from_reader(&mut r)?)
}

pub fn from_onnx(proto: pb::ModelProto) -> Result<Model> {
    let mut nodes = vec![];
    let mut outlets_index: HashMap<String, OutletId> = HashMap::new();
    let mut nodes_by_name: HashMap<String, usize> = HashMap::new();
    let op_builder = ::ops::OpBuilder::new();
    let graph = proto.get_graph();
    for input in graph.get_input().iter() {
        outlets_index.insert(input.get_name().to_owned(), OutletId::new(nodes.len(), 0));
        let fact = TensorFact::from_pb(input.get_field_type().get_tensor_type())?;
        let placeholder = Node {
            id: nodes.len(),
            name: input.get_name().to_owned(),
            op: Box::new(::ops::placeholder::Placeholder::new(fact)),
            op_name: "Placeholder".to_string(),
            inputs: vec![],
        };
        nodes_by_name.insert(input.get_name().to_owned(), nodes.len());
        nodes.push(placeholder);
    }
    for pbnode in graph.get_node().iter() {
        let name = if pbnode.get_name() != "" {
            pbnode.get_name().to_string()
        } else if pbnode.get_output().len() > 0 && pbnode.get_output()[0] != "" {
            pbnode.get_output()[0].to_owned()
        } else {
            format!("{}-{}", nodes.len(), pbnode.get_op_type())
        };
        for (ix, output) in pbnode.get_output().iter().enumerate() {
            outlets_index.insert(output.to_string(), OutletId::new(nodes.len(), ix));
        }
        let op_name = pbnode.get_op_type().to_owned();
        let node = Node {
            id: nodes.len(),
            name: name.clone(),
            op: super::ops::build(&*op_name),
            op_name,
            inputs: vec![],
        };
        nodes_by_name.insert(name, nodes.len());
        nodes.push(node)
    }
    for (pbnode, mut node) in graph
        .get_node()
        .iter()
        .zip(&mut nodes.iter_mut().skip(graph.get_input().len()))
    {
        for pbinput in pbnode.get_input() {
            node.inputs.push(
                outlets_index
                    .get(pbinput)
                    .ok_or_else(|| format!("Can not find matching outlet for {}", pbinput))?
                    .clone(),
            )
        }
    }
    Ok(Model(Arc::new(RawModel {
        nodes,
        nodes_by_name,
    })))
}

#[cfg(test)]
mod tests {
    use std::{fs, path};

    #[test]
    fn onnx_abs() {
        let root = path::PathBuf::from("test_abs");
        let model = ::onnx::for_path(root.join("model.onnx")).unwrap();
        for d in fs::read_dir(root).unwrap() {
            let d = d.unwrap();
            if d.metadata().unwrap().is_dir()
                && d.file_name()
                    .to_str()
                    .unwrap()
                    .starts_with("test_data_set_")
            {
            }
        }
        panic!();
    }
}
