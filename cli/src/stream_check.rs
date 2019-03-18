use crate::display_graph;
use crate::{CliResult, Parameters, SomeModel};
use ndarray::ArrayD;
use ndarray::Axis;
use tract_core::model::{OutletId, TensorInfo};
use tract_core::plan::{SimplePlan, SimpleState};

pub fn handle(params: Parameters, options: display_graph::DisplayOptions) -> CliResult<()> {
    let (fixed, pulsed) = if let SomeModel::Pulsed(n, p) = params.tract_model {
        (n, p)
    } else {
        unreachable!();
    };

    let fixed_input_fact = fixed.input_fact()?;
    let pulsed_input_fact = pulsed.input_fact()?;
    let pulse = pulsed_input_fact.pulse();

    // First generate random values for the inputs.
    let display_graph =
        display_graph::DisplayGraph::from_model_and_options(pulsed.clone(), options)?
            .with_graph_def(&params.graph)?;

    let eval_order = ::tract_core::model::eval_order(&fixed)?;

    for &fixed_node in eval_order.iter() {
        let pulsed_node = match pulsed.node_by_name(&*fixed.node(fixed_node).name) {
            Ok(node) => node.id,
            _ => continue,
        };
        for output in 0..fixed.node(fixed_node).outputs.len() {
            let fixed_outlet = OutletId::new(fixed_node, output);
            let pulsed_outlet = OutletId::new(pulsed_node, output);

            let mut pulsed = pulsed.clone();
            pulsed.set_output_outlets(&[pulsed_outlet])?;

            let pulsed_output_fact = pulsed.output_fact()?;
            let output_pulse = pulsed_output_fact.pulse();
            let output_axis = pulsed_output_fact.axis;
            let delay = pulsed_output_fact.delay;

            let stream_dim = delay + 3 * output_pulse + output_pulse / 2;

            let fixed_input = crate::tensor::tensor_for_fact(
                &fixed_input_fact.to_tensor_fact(),
                Some(stream_dim),
            )?;

            let mut fixed = fixed.clone();
            fixed.set_output_outlets(&[fixed_outlet])?;
            let fixed_result = SimplePlan::new(&fixed)?.run(tvec!(fixed_input.clone()))?.remove(0);
            let fixed_result = fixed_result.to_array_view::<f32>()?;
            let fixed_output_len = fixed_result.shape()[output_axis];

            let plan = SimplePlan::new(&pulsed)?;
            let mut state = SimpleState::new(&plan)?;

            for i in 0.. {
                let mut pulsed_input = ArrayD::from_elem(&*pulsed_input_fact.shape, std::f32::NAN);
                let offset = i * pulse;
                if offset < stream_dim {
                    let count = pulse.min(stream_dim - offset);
                    pulsed_input
                        .slice_axis_mut(Axis(pulsed_input_fact.axis), (0..count).into())
                        .assign(&fixed_input.to_array_view::<f32>()?.slice_axis(
                            Axis(pulsed_input_fact.axis),
                            (offset..offset + count).into(),
                        ));
                };
                // FIXME set session state!
                let output = state.run(tvec!(pulsed_input.into()))?.remove(0);
                let output = output.to_array_view::<f32>()?;
                let output_offset = i * output_pulse;
                let (f_o, p_o, count) = if output_offset + output_pulse < delay {
                    // entire pulse before signal, wait
                    continue;
                } else if output_offset >= delay + fixed_output_len {
                    // entire pulse after signal, we stop
                    break;
                } else if output_offset < delay {
                    // beginning of signal
                    let count = pulse + output_offset - delay;
                    (0, output_pulse - count, count)
                } else if output_offset + output_pulse > delay + fixed_output_len {
                    // end of signal
                    let count = fixed_output_len + delay - output_offset;
                    (output_offset - delay, 0, count)
                } else {
                    (output_offset - delay, 0, output_pulse)
                };
                let valid_pulse_result =
                    output.slice_axis(Axis(output_axis), (p_o..p_o + count).into());
                let valid_fixed_result =
                    fixed_result.slice_axis(Axis(output_axis), (f_o..f_o + count).into());
                if valid_pulse_result != valid_fixed_result {
                    display_graph.render_node(pulsed.node(pulsed_node))?;
                    bail!("Error checking pulse mode")
                }
            }
        }
    }

    Ok(())
}
