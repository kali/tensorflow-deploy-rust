use std::thread;
use simplelog::Level::Info;

use ndarray::Axis;
use { Parameters, InputData };
use errors::*;
use utils::random_tensor;
use profile::ProfileData;
use rusage::{ Duration, Instant };
use format::*;

/// Handles the `profile` subcommand when there are streaming dimensions.
pub fn handle(params: Parameters, input: InputData, _max_iters: u64, _max_time: u64) -> Result<()> {
    use Tensor::*;

    use tfdeploy::StreamingInput;
    use tfdeploy::StreamingState;

    let model = params.tfd_model.clone();
    let datatype = input.datatype;
    let shape = input.shape;

    let axis = shape.iter()
        .position(|&d| d == None)
        .unwrap();
    let inputs = params.inputs.iter()
        .map(|&s| (s, StreamingInput::Streamed(datatype, shape.clone())))
        .collect::<Vec<_>>();

    info!("Initializing the StreamingState.");
    let start = Instant::now();
    let state = StreamingState::start(
        model.clone(),
        inputs.clone(),
        Some(params.output)
    )?;

    let measure = Duration::since(&start, 1);
    let mut states = (0..100).map(|_| state.clone()).collect::<Vec<_>>();

    info!("Initialized the StreamingState in: {}", dur_avg_oneline(measure));

    if log_enabled!(Info) {
        println!();
        print_header(format!("Streaming profiling for {}:", params.name), "white");
    }

    // Either get the input data from the input file or generate it randomly.
    let random_shape = shape.iter()
        .map(|d| d.unwrap_or(20))
        .collect::<Vec<_>>();
    let data = input.data
        .unwrap_or_else(|| random_tensor(random_shape, datatype));

    // Split the input data into chunks along the streaming axis.
    macro_rules! split_inner {
        ($constr:ident, $array:expr) => ({
            $array.axis_iter(Axis(axis))
                .map(|v| $constr(v.insert_axis(Axis(axis)).to_owned()))
                .collect::<Vec<_>>()
        })
    }

    let chunks = match data {
        F64(m) => split_inner!(F64, m),
        F32(m) => split_inner!(F32, m),
        I32(m) => split_inner!(I32, m),
        I8(m) => split_inner!(I8, m),
        U8(m) => split_inner!(U8, m),
        String(m) => split_inner!(String, m),
    };

    let mut profile = ProfileData::new(&params.graph, &state.model());

    for (step, chunk) in chunks.into_iter().enumerate() {
        for &input in &params.inputs {
            trace!("Starting step {:?} with input {:?}.", step, chunk);

            let mut input_chunks = vec![Some(chunk.clone()); 100];
            let mut outputs = Vec::with_capacity(100);
            let start = Instant::now();

            for i in 0..100 {
                outputs.push(states[i].step_wrapping_ops(input, input_chunks[i].take().unwrap(),
                    |node, input, buffer| {
                        let start = Instant::now();
                        let r = node.op.step(input, buffer)?;
                        profile.add(node.id, Duration::since(&start, 1))?;
                        Ok(r)
                    }
                ));
            }

            let measure = Duration::since(&start, 100);
            println!("Completed step {:2} with output {:?} in: {}", step, outputs[0], dur_avg_oneline(measure));
            thread::sleep(::std::time::Duration::from_secs(1));
        }
    }

    println!();
    print_header(format!("Summary for {}:", params.name), "white");

    profile.print_most_consuming_nodes(None)?;
    println!();

    profile.print_most_consuming_ops();

    Ok(())
}

