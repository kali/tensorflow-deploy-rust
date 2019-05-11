use crate::internal::*;
use crate::model::*;
use std::ops::{Deref, DerefMut};

#[derive(Clone, Debug)]
pub struct ModelPatch<TI: TensorInfo> {
    pub model: Model<TI>,
    incoming: HashMap<OutletId, OutletId>,
    shunt_outlet_by: HashMap<OutletId, OutletId>,
}

impl<TI: TensorInfo> Default for ModelPatch<TI> {
    fn default() -> ModelPatch<TI> {
        ModelPatch {
            model: Model::default(),
            incoming: HashMap::new(),
            shunt_outlet_by: HashMap::new(),
        }
    }
}

impl<TI: TensorInfo> Deref for ModelPatch<TI> {
    type Target = Model<TI>;
    fn deref(&self) -> &Model<TI> {
        &self.model
    }
}

impl<TI: TensorInfo> DerefMut for ModelPatch<TI> {
    fn deref_mut(&mut self) -> &mut Model<TI> {
        &mut self.model
    }
}

impl<TI: TensorInfo> ModelPatch<TI> {
    pub fn tap_model(&mut self, model: &Model<TI>, outlet: OutletId) -> TractResult<OutletId> {
        let fact = model.outlet_fact(outlet)?;
        let node_id = self
            .add_source(format!("incoming-{}/{}", outlet.node, outlet.slot), objekt::clone(fact))?;
        let inside = OutletId::new(node_id, 0);
        self.incoming.insert(inside, outlet);
        Ok(inside)
    }

    pub fn shunt_outside(&mut self, outlet: OutletId, by: OutletId) -> TractResult<()> {
        self.shunt_outlet_by.insert(outlet, by);
        Ok(())
    }

    pub fn replace_single_op<O: Into<Box<Op>>>(
        patched_model: &Model<TI>,
        node: &Node<TI>,
        inputs: &[OutletId],
        new_op: O,
    ) -> TractResult<ModelPatch<TI>> {
        let mut patch = ModelPatch::default();
        let new_op = new_op.into();
        let outputs = node.outputs.iter().map(|o| objekt::clone(&o.fact)).collect();
        let by = patch.add_node(&*node.name, new_op, outputs)?;
        for (ix, i) in inputs.iter().enumerate() {
            let o = patch.tap_model(&patched_model, *i)?;
            patch.add_edge(o, InletId::new(by, ix))?;
        }
        for ix in 0..node.outputs.len() {
            patch.shunt_outside(OutletId::new(node.id, ix), OutletId::new(by, ix))?;
        }
        Ok(patch)
    }

    pub fn single_unary_op<O: Into<Box<Op>>>(
        patched_model: &Model<TI>,
        node: &Node<TI>,
        new_op: O,
    ) -> TractResult<ModelPatch<TI>> {
        Self::replace_single_op(patched_model, node, &[node.inputs[0]], new_op)
    }

    pub fn apply(self, model: &mut Model<TI>) -> TractResult<()> {
        let ModelPatch { model: patch, incoming: mut mapping, shunt_outlet_by } = self;
        for node in patch.nodes {
            if node.op_is::<crate::ops::source::Source>() {
                continue;
            }
            let Node { id, name, inputs, op, outputs } = node;
            let n_outputs = outputs.len();
            let facts = outputs.into_iter().map(|of| of.fact).collect();
            let added_node_id = model.add_node_disable_output_guess(name, op, facts, true)?;
            for (ix, input) in inputs.into_iter().enumerate() {
                model.add_edge(mapping[&input], InletId::new(added_node_id, ix))?;
            }
            for ix in 0..n_outputs {
                mapping.insert(OutletId::new(id, ix), OutletId::new(added_node_id, ix));
            }
        }
        for (outlet, by) in shunt_outlet_by {
            let fixed_by = mapping[&by];
            let succs = model.nodes()[outlet.node].outputs[outlet.slot].successors.clone();
            for succ in succs {
                model.add_edge(fixed_by, succ)?;
            }
            for o in model.outputs.iter_mut() {
                if *o == outlet {
                    *o = fixed_by;
                }
            }
        }
        Ok(())
    }
}
