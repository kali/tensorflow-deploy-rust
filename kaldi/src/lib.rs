#[macro_use]
extern crate derive_new;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate tract_core;

pub mod model;
mod ops;
pub mod parser;

pub use model::Kaldi;
pub use model::KaldiProtoModel;

pub fn kaldi() -> Kaldi {
    let mut kaldi = Kaldi::default();
    ops::register_all_ops(&mut kaldi.op_register);
    kaldi
}
