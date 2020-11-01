use crate::internal::*;
use crate::model::*;
use crate::ops;
use crate::ops::invariants;
use crate::plan::{SimplePlan, SimpleState};
use crate::TractResult;

/// A model with completely determined types and shapes.
pub type TypedModel = Graph<TypedFact, Box<dyn TypedOp>>;
/// Node for TypedModel graph
pub type TypedNode = Node<TypedFact, Box<dyn TypedOp>>;
/// A ModelPatch for TypedModel.
pub type TypedModelPatch = ModelPatch<TypedFact, Box<dyn TypedOp>>;
/// An execution plan for TypedModel.
pub type TypedSimplePlan<M> = SimplePlan<TypedFact, Box<dyn TypedOp>, M>;
/// A runnable TypedModel (new name for SimplePlan).
pub type TypedRunnableModel<M> = RunnableModel<TypedFact, Box<dyn TypedOp>, M>;
/// An execution state for TypedModel.
pub type TypedSimpleState<M, P> = SimpleState<TypedFact, Box<dyn TypedOp>, M, P>;

/// A runnable model with fixed inputs and outputs.
pub type RunnableModel<F, O, M> = SimplePlan<F, O, M>;

impl SpecialOps<TypedFact, Box<dyn TypedOp>> for TypedModel {
    fn is_source(op: &Box<dyn TypedOp>) -> bool {
        op.as_op().downcast_ref::<ops::source::TypedSource>().is_some()
    }

    fn create_dummy(&self) -> Box<dyn TypedOp> {
        Box::new(crate::ops::dummy::Dummy::new())
    }

    fn create_source(&self, fact: TypedFact) -> Box<dyn TypedOp> {
        Box::new(crate::ops::source::TypedSource::new(fact))
    }

    fn wire_node(
        &mut self,
        name: impl Into<String>,
        op: impl Into<Box<dyn TypedOp>>,
        inputs: &[OutletId],
        ) -> TractResult<TVec<OutletId>> {
        let op = op.into();
        let name = name.into();
        let output_facts = {
            let input_facts =
                inputs.iter().map(|o| self.outlet_fact(*o)).collect::<TractResult<TVec<_>>>()?;
            if input_facts.iter().all(|f| f.konst.is_some()) && op.is_stateless() {
                let tensors =
                    input_facts.iter().map(|f| f.konst.clone().unwrap()).collect::<TVec<_>>();
                let outputs = op.eval(tensors)?;
                outputs.into_iter().map(|t| TypedFact::from(t)).collect()
            } else {
                op.output_facts(&*input_facts)
                    .with_context(|| format!("wiring {} ({:?})", name, op))?
            }
        };
        let id = self.add_node(name, op, output_facts)?;
        inputs
            .iter()
            .enumerate()
            .try_for_each(|(ix, i)| self.add_edge(*i, InletId::new(id, ix)))?;
        Ok(self.node(id).outputs.iter().enumerate().map(|(ix, _)| OutletId::new(id, ix)).collect())
    }
}

impl TypedModel {
    pub fn signature(&self) -> u64 {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    pub fn into_optimized(self) -> TractResult<TypedModel> {
        self.declutter()?.optimize()
    }

    #[cfg(all(debug_assertions, feature = "paranoid_assertions"))]
    pub fn check_consistent_facts(&self) -> TractResult<()> {
        for node_id in &self.eval_order()? {
            let input_facts = self.node_input_facts(*node_id)?;
            let node = &self.nodes[*node_id];
            if node.id != *node_id {
                bail!("Node at position {} has id {}", node_id, node.id);
            }
            let output_facts = node.op.output_facts(&input_facts)?;
            if node.outputs.len() != output_facts.len() {
                bail!(
                    "Inconsistent model, node output count mismatch. Op says {}, node says {}. {}",
                    output_facts.len(),
                    node.outputs.len(),
                    node
                    );
            }
            if node
                .outputs
                    .iter()
                    .map(|o| &o.fact)
                    .zip(output_facts.iter())
                    .any(|(a, b)| a.datum_type != b.datum_type || a.shape != b.shape)
                    {
                        bail!(
                            "Inconsistent model, node output types mismatch. Op says: {:?}, node says: {:?}. {} with inputs {:?}",
                            output_facts, node.outputs.iter().map(|o| &o.fact).collect::<Vec<_>>(), node, input_facts)
                    }
        }
        for node in &self.nodes {
            for (ix, output) in node.outputs.iter().enumerate() {
                output.fact.consistent().with_context(|| {
                    format!("Inconsistent fact {:?}: {:?}", OutletId::new(node.id, ix), output.fact)
                })?
            }
        }
        Ok(())
    }

    fn optimize_passes(
        &self,
        passes: &mut [Box<dyn crate::optim::TypedPass>],
        ) -> TractResult<TypedModel> {
        #[cfg(all(debug_assertions, feature = "paranoid_assertions"))]
        {
            self.check_consistent_facts()?;
        }
        let mut model = self.clone();
        let mut patches = 0;
        for i in 0.. {
            model = model.compact()?;
            let mut done_something_this_time = false;
            'pass: for p in passes.iter_mut() {
                loop {
                    let mut done_something_this_pass = false;
                    let mut seen = std::collections::HashSet::new();
                    p.reset()?;
                    while let Some(mut patch) = p.next(&model)? {
                        patch.push_context(format!("{:?}/{}", p, i));
                        #[cfg(all(debug_assertions, feature = "paranoid_assertions"))]
                        {
                            patch.model.check_consistent_facts()?;
                            model.check_consistent_facts()?;
                            patch.model.invariants()?;
                            model.invariants()?;
                        }
                        if let Some(watchdog) = patch.dont_apply_twice.take() {
                            if !seen.contains(&watchdog) {
                                debug!("Loop detected: {} seen before", watchdog);
                                break 'pass;
                            } else {
                                seen.insert(watchdog);
                            }
                        }
                        debug!("applying patch #{}: {}", patches, patch.context.iter().rev().join(" >> "),);
                        done_something_this_pass = true;
                        done_something_this_time = true;
                        patch.apply(&mut model)?;
                        seen.clear();
                        patches += 1
                    }
                    #[cfg(all(debug_assertions, feature = "paranoid_assertions"))]
                    {
                        model.check_edges()?;
                        model
                            .check_consistent_facts()
                            .with_context(|| format!("after declutter pass {:?}", p))?
                    }
                    if !done_something_this_pass {
                        continue 'pass;
                    }
                }
            }
            if !done_something_this_time {
                return Ok(model);
            }
            model = model.compact()?;
        }
        unreachable!()
    }

    /// Perform declutter passes on the network.
    pub fn declutter(&self) -> TractResult<TypedModel> {
        self.optimize_passes(&mut crate::optim::declutter())
    }

    pub fn concretize_dims(&self, values: &SymbolValues) -> TractResult<TypedModel> {
        use crate::model::translator::Translate;
        impl Translate<TypedFact, Box<dyn TypedOp>, TypedFact, Box<dyn TypedOp>> for SymbolValues {
            fn translate_node(
                &self,
                source: &TypedModel,
                node: &TypedNode,
                target: &mut TypedModel,
                mapping: &HashMap<OutletId, OutletId>,
                ) -> TractResult<TVec<OutletId>> {
                node.op.concretize_dims(source, node, target, mapping, self)
            }
        }
        values.translate_model(&self)
    }

    /// Translate the graph to locally optimized operators (LIR or MIR ops).
    pub fn optimize(self) -> TractResult<TypedModel> {
        self.optimize_passes(&mut crate::optim::codegen())
    }

    pub fn invariants(&self) -> TractResult<invariants::Invariants> {
        invariants::for_model(self)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test() {
        fn is_sync<T: Sync>() {}
        is_sync::<TypedModel>();
    }
}
