extern crate bincode;
#[macro_use]
extern crate clap;
extern crate colored;
#[cfg(feature = "tensorflow")]
extern crate conform;
extern crate dot;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;
extern crate ndarray;
#[macro_use]
extern crate prettytable;
extern crate rand;
extern crate simplelog;
extern crate terminal_size;
extern crate textwrap;
#[macro_use]
extern crate tfdeploy;
extern crate pbr;
extern crate atty;
extern crate libc;
#[macro_use]
extern crate rouille;
extern crate open;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use std::fs::File;
use std::io::Read;
use std::process;

use simplelog::Level::{Error, Trace};
use simplelog::{Config, LevelFilter, TermLogger};
use tfdeploy::tfpb;
use tfdeploy::Tensor;
use tfpb::graph::GraphDef;
use tfpb::types::DataType;

use errors::*;
#[allow(unused_imports)]
use format::Row;

mod analyse;
mod compare;
mod dump;
mod errors;
mod format;
mod graphviz;
mod utils;
mod profile;
mod prune;
mod rusage;
mod web;

/// The default maximum for iterations and time.
const DEFAULT_MAX_ITERS: u64 = 100_000;
const DEFAULT_MAX_TIME: u64 = 200;

/// Structure holding the input data.
pub struct InputData {
    data: Option<Tensor>,
    shape: Vec<Option<usize>>,
    datatype: DataType,
}

/// Structure holding the parsed parameters.
pub struct Parameters {
    name: String,
    graph: GraphDef,
    tfd_model: tfdeploy::Model,

    #[cfg(feature = "tensorflow")]
    tf_model: conform::tf::Tensorflow,

    input: Option<InputData>,
    inputs: Vec<usize>,
    output: usize,
}

/// Entrypoint for the command-line interface.
fn main() {
    let app = clap_app!(("tfdeploy-cli") =>
        (version: "1.0")
        (author: "Romain Liautaud <romain.liautaud@snips.ai>")
        (about: "A set of tools to compare tfdeploy with tensorflow.")

        (@setting UnifiedHelpMessage)
        (@setting SubcommandRequired)
        (@setting DeriveDisplayOrder)

        (@arg model: +required +takes_value
            "Sets the TensorFlow model to use (in Protobuf format).")

        (@arg inputs: -i --input ... [input]
            "Sets the input nodes names (auto-detects otherwise).")

        (@arg output: -o --output [output]
            "Sets the output node name (auto-detects otherwise).")

        (@arg size: -s --size [size]
            "Generates random input of a given size, e.g. 32x64xf32.")

        (@arg data: -f --data [data]
            "Loads input data from a given file.")

        (@arg verbosity: -v ... "Sets the level of verbosity.")

        (@subcommand compare =>
            (about: "Compares the output of tfdeploy and tensorflow on randomly generated input."))

        (@subcommand dump =>
            (about: "Dumps the Tensorflow graph in human readable form.")
            (@arg web: --web
                "Displays the dump in a web interface."))

        (@subcommand profile =>
            (about: "Benchmarks tfdeploy on randomly generated input.")
            (@arg max_iters: -n [max_iters]
                "Sets the maximum number of iterations for each node [default: 10_000].")
            (@arg max_time: -t [max_time]
                "Sets the maximum execution time for each node (in ms) [default: 500]."))

        (@subcommand analyse =>
            (about: "Analyses the graph to infer properties about tensors (experimental).")
            (@arg prune: --prune
                "Prunes constant nodes and edges from the graph.")
            (@arg web: --web
                "Displays the results of the analysis in a web interface."))
    );

    let matches = app.get_matches();

    // Configure the logging level.
    let level = match matches.occurrences_of("verbosity") {
        0 => LevelFilter::Warn,
        1 => LevelFilter::Info,
        2 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };

    TermLogger::init(
        level,
        Config {
            time: None,
            time_format: None,
            level: Some(Error),
            target: None,
            location: Some(Trace),
        },
    ).unwrap();

    if let Err(e) = handle(matches) {
        error!("{}", e.to_string());
        process::exit(1)
    }
}

/// Handles the command-line input.
fn handle(matches: clap::ArgMatches) -> Result<()> {
    let params = parse(&matches)?;

    match matches.subcommand() {
        ("compare", _) => compare::handle(params),

        ("dump", Some(m)) => dump::handle(
            params,
            m.is_present("web")
        ),

        ("profile", Some(m)) => profile::handle(
            params,
            match m.value_of("max_iters") {
                None => DEFAULT_MAX_ITERS,
                Some(s) => s.parse::<u64>()?,
            },
            match m.value_of("max_time") {
                None => DEFAULT_MAX_TIME,
                Some(s) => s.parse::<u64>()?,
            },
        ),

        ("analyse", Some(m)) => analyse::handle(
            params,
            m.is_present("prune"),
            m.is_present("web")
        ),

        (s, _) => bail!("Unknown subcommand {}.", s),
    }
}

/// Parses the command-line arguments.
fn parse(matches: &clap::ArgMatches) -> Result<Parameters> {
    let name = matches.value_of("model").unwrap();
    let graph = tfdeploy::Model::graphdef_for_path(&name)?;
    let tfd_model = tfdeploy::for_path(&name)?;

    #[cfg(feature = "tensorflow")]
    let tf_model = conform::tf::for_path(&name)?;

    let input = match (matches.value_of("size"), matches.value_of("data")) {
        (Some(size), None)     => Some(parse_size(size)?),
        (None, Some(filename)) => Some(parse_data(filename)?),
        _ => None
    };

    let inputs = match matches.values_of("inputs") {
        Some(names) => names
            .map(|s| Ok(tfd_model.node_id_by_name(s)?))
            .collect::<Result<_>>()?,
        None => tfdeploy::analyser::detect_inputs(&tfd_model)?
            .ok_or("Impossible to auto-detect input nodes: no placeholder.")?,
    };

    let output = match matches.value_of("output") {
        Some(name) => tfd_model.node_id_by_name(name)?,
        None => tfdeploy::analyser::detect_output(&tfd_model)?.ok_or("Impossible to auto-detect output nodes.")?,
    };

    #[cfg(feature = "tensorflow")]
    return Ok(Parameters {
        name: name.to_string(),
        graph,
        tfd_model,
        tf_model,
        inputs,
        output,
        input,
    });

    #[cfg(not(feature = "tensorflow"))]
    return Ok(Parameters {
        name: name.to_string(),
        graph,
        tfd_model,
        inputs,
        output,
        input,
    });
}

/// Parses the `size` command-line argument.
fn parse_size(size: &str) -> Result<InputData> {
    let splits = size.split("x").collect::<Vec<_>>();

    if splits.len() < 1 {
        bail!("The <size> argument should be formatted as {size}x{...}x{type}.");
    }

    let (datatype, shape) = splits.split_last().unwrap();

    let shape = shape
        .iter()
        .map(|s| match *s {
            "S" => Ok(None),            // Streaming dimension.
            _   => Ok(Some(s.parse()?)) // Regular dimension.
        })
        .collect::<Result<Vec<_>>>()?;

    if shape.iter().filter(|o| o.is_none()).count() > 1 {
        bail!("The <size> argument doesn't support more than one streaming dimension.");
    }

    let datatype = match datatype.to_lowercase().as_str() {
        "f64" => DataType::DT_DOUBLE,
        "f32" => DataType::DT_FLOAT,
        "i32" => DataType::DT_INT32,
        "i8" => DataType::DT_INT8,
        "u8" => DataType::DT_UINT8,
        _ => bail!("Type of the input should be f64, f32, i32, i8 or u8."),
    };

    Ok(InputData { data: None, shape, datatype })
}


/// Parses the `data` command-line argument.
fn parse_data(filename: &str) -> Result<InputData> {
    let mut file = File::open(filename)?;
    let mut data = String::new();
    file.read_to_string(&mut data)?;

    let mut lines = data.lines();
    let InputData { shape, datatype, .. } = parse_size(lines.next().unwrap())?;

    let values = lines
        .flat_map(|l| l.split_whitespace())
        .collect::<Vec<_>>();

    // We know there is at most one streaming dimension, so we can deduce the
    // missing value with a simple division.
    let product: usize =  shape.iter().map(|o| o.unwrap_or(1)).product();
    let missing = values.len() / product;
    let data_shape = shape.iter()
        .map(|o| o.unwrap_or(missing))
        .collect::<Vec<_>>();

    macro_rules! for_type {
        ($t:ty) => ({
            let array = ndarray::Array::from_iter(
                values.iter().map(|v| v.parse::<$t>().unwrap())
            );

            array.into_shape(data_shape)?
        });
    }

    let tensor = match datatype {
        DataType::DT_DOUBLE => for_type!(f64).into(),
        DataType::DT_FLOAT => for_type!(f32).into(),
        DataType::DT_INT32 => for_type!(i32).into(),
        DataType::DT_INT8 => for_type!(i8).into(),
        DataType::DT_UINT8 => for_type!(u8).into(),
        _ => unimplemented!(),
    };

    Ok(InputData { data: Some(tensor), shape, datatype })
}

