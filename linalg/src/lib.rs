extern crate lazy_static;
extern crate libc;
extern crate log;
#[macro_use]
extern crate objekt;
#[cfg(test)]
extern crate matrixmultiply;
extern crate num_traits;
#[cfg(test)]
extern crate proptest;

pub mod align;
pub mod f16;
pub mod frame;
mod generic;

#[cfg(target_arch = "x86_64")]
pub mod x86_64_fma;

#[cfg(any(target_arch = "arm", target_arch = "armv7"))]
pub mod arm32;

pub use self::frame::{MatMul, PackedMatMul};

pub struct Ops {
    pub smm: Box<Fn(usize, usize, usize) -> Box<MatMul<f32>> + Send + Sync>,
    pub dmm: Box<Fn(usize, usize, usize) -> Box<MatMul<f64>> + Send + Sync>,
}

pub fn generic() -> Ops {
    Ops {
        smm: Box::new(|m, k, n| Box::new(PackedMatMul::<generic::SMatMul4x4, f32>::new(m, k, n))),
        dmm: Box::new(|m, k, n| Box::new(PackedMatMul::<generic::DMatMul4x2, f64>::new(m, k, n))),
    }
}

#[allow(unreachable_code)]
pub fn best() -> Ops {
    let mut ops = generic();
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("fma") {
            log::info!("x86_64/fma activated for smm");
            ops.smm = Box::new(|m, k, n| {
                Box::new(PackedMatMul::<x86_64_fma::matmul::KerFma16x6, f32>::new(
                    m, k, n,
                ))
            });
        }
    }
    #[cfg(any(target_arch = "arm", target_arch = "armv7"))]
    arm32::plug(&mut ops);
    return ops;
}

lazy_static::lazy_static! {
    static ref OPS: Ops = {
        best()
    };
}

pub fn ops() -> &'static Ops {
    &*OPS
}
