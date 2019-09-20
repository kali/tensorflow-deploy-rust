use super::Downsample;
use crate::internal::*;
use crate::ops;

pub fn pull_downsample_over_slice<D: DimLike>(
    model: &TypedModel,
    slice_node: &TypedNode,
    slice_op: &ops::array::Slice<D>,
    down_node: &TypedNode,
    down_op: &Downsample,
) -> TractResult<Option<TypedModelPatch>>
where
    TDim: From<D>,
{
    let modulo = (down_op.modulo + slice_op.start.to_integer()? as usize) % down_op.stride;
    let left = (down_op.modulo + slice_op.start.to_integer()? as usize) / down_op.stride;
    let mut patch = TypedModelPatch::default();
    let tap = patch.tap_model(model, slice_node.inputs[0])?;
    let final_len = down_node.outputs[0].fact.shape.dim(down_op.axis);
    let new_down = Downsample::new(down_op.axis, down_op.stride, modulo);
    let ds = patch.wire_node(&*down_node.name, new_down, [tap].as_ref())?;
    let new_start = left;
    let new_end = (final_len.to_dim() + left).to_integer()? as usize;
    let new_slice = patch.wire_node(
        &*slice_node.name,
        ops::array::Slice::new(slice_op.axis, new_start, new_end),
        &*ds,
    )?[0];
    patch.shunt_outside(OutletId::new(down_node.id, 0), new_slice)?;
    return Ok(Some(patch));
}

pub fn pull_downsample_over_adddims(
    model: &TypedModel,
    add_node: &TypedNode,
    add_op: &ops::array::AddDims,
    down_node: &TypedNode,
    down_op: &Downsample,
) -> TractResult<Option<TypedModelPatch>> {
    let mut patch = TypedModelPatch::default();
    let tap = patch.tap_model(model, add_node.inputs[0])?;
    let mut new_down = down_op.clone();
    new_down.axis -= add_op.axes.iter().filter(|&ax| *ax <= down_op.axis).count();
    let ds = patch.wire_node(&*down_node.name, new_down, [tap].as_ref())?;
    let add = patch.wire_node(&*add_node.name, add_op.clone(), &*ds)?;
    patch.shunt_outside(OutletId::new(down_node.id, 0), add[0])?;
    return Ok(Some(patch));
}

pub fn pull_downsample_over_rmdims(
    model: &TypedModel,
    rm_node: &TypedNode,
    rm_op: &ops::array::RmDims,
    down_node: &TypedNode,
    down_op: &Downsample,
) -> TractResult<Option<TypedModelPatch>> {
    let mut patch = TypedModelPatch::default();
    let tap = patch.tap_model(model, rm_node.inputs[0])?;
    let mut new_down = down_op.clone();
    new_down.axis += rm_op.axes.iter().filter(|&ax| *ax <= down_op.axis).count();
    let ds = patch.wire_node(&*down_node.name, new_down, [tap].as_ref())?;
    let rm = patch.wire_node(&*rm_node.name, rm_op.clone(), &*ds)?;
    patch.shunt_outside(OutletId::new(down_node.id, 0), rm[0])?;
    return Ok(Some(patch));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops;
    use proptest::prelude::*;
    use proptest::test_runner::TestCaseResult;

    fn crop_then_down_strat() -> BoxedStrategy<(usize, usize, usize, usize, usize)> {
        (1usize..5, 1usize..5)
            .prop_flat_map(|(cropped, stride)| {
                (Just(cropped), 0..=cropped, Just(stride), (cropped + 15)..=(cropped + 15))
            })
            .prop_flat_map(|(cropped, left, stride, len)| {
                (Just(len), Just(left), Just(cropped - left), Just(stride), 0..stride)
            })
            .boxed()
    }

    fn crop_then_down(
        len: usize,
        left: usize,
        right: usize,
        stride: usize,
        modulo: usize,
    ) -> TestCaseResult {
        let _ = env_logger::Builder::from_env("TRACT_LOG").try_init();
        let model = {
            let mut model = InferenceModel::default();
            let input = model.add_source("input", TensorFact::dt_shape(i32::datum_type(), &[len]))?;
            let crop = model.wire_node("crop", ops::array::Slice::new(0, left, len - right), &[input])?;
            let down = model.wire_node("down", Downsample::new(0, stride, modulo), &crop)?;
            model.set_output_outlets(&down)?;
            model
        };
        trace!("{:#?}", model);
        prop_assert!(model.node(model.output_outlets().unwrap()[0].node).op_is::<Downsample>());
        let typed = model.into_typed()?;
        trace!("{:#?}", typed);
        let input = tensor1(&(0i32..len as _).collect::<Vec<_>>());
        let expected = SimplePlan::new(&typed)?.run(tvec!(input.clone()))?;

        info!("Decluttering");
        let typed = typed.declutter()?;
        trace!("{:#?}", typed);
        let order = typed.eval_order()?;
        prop_assert!(
            typed.node(order[1]).op_is::<Downsample>()
                || !typed.nodes().iter().any(|n| n.op_is::<Downsample>())
        );
        let found = SimplePlan::new(&typed)?.run(tvec!(input))?;
        prop_assert_eq!(found, expected);
        Ok(())
    }

    proptest! {
        #[test]
        fn crop_then_down_prop((len, left, right, stride, modulo) in crop_then_down_strat()) {
            crop_then_down(len, left, right, stride, modulo).unwrap()
        }
    }

    #[test]
    fn crop_then_down_1() {
        crop_then_down(1, 0, 0, 2, 0).unwrap()
    }

    #[test]
    fn crop_then_down_2() {
        crop_then_down(2, 0, 1, 2, 0).unwrap()
    }

    #[test]
    fn crop_then_down_3() {
        crop_then_down(0, 0, 0, 2, 1).unwrap()
    }

    #[test]
    fn crop_then_down_4() {
        crop_then_down(1, 0, 1, 2, 1).unwrap()
    }

    #[test]
    fn crop_then_down_5() {
        crop_then_down(16, 0, 1, 2, 1).unwrap()
    }
}
