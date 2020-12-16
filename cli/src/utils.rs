use crate::CliResult;
use tract_hir::internal::*;

/// Compares the outputs of a node in tract and tensorflow.
pub fn check_outputs(got: &[Arc<Tensor>], expected: &[Option<Arc<Tensor>>]) -> CliResult<()> {
    if got.len() != expected.len() {
        bail!("Number of output differ: got:{}, expected:{}", got.len(), expected.len())
    }

    for (ix, (got, exp)) in got.iter().zip(expected.iter()).enumerate() {
        if let Some(exp) = exp {
            exp.close_enough(got, true).with_context(|| {
                format!("Checking output {} (expected {:?}, got {:?}", ix, exp, got)
            })?;
            info!("Checked output #{}, ok.", ix);
        }
    }

    Ok(())
}

/// Compares the outputs of a node in tract and tensorflow.
pub fn check_inferred(got: &[InferenceFact], expected: &[InferenceFact]) -> CliResult<()> {
    if got.len() != expected.len() {
        bail!("Number of output differ: got:{}, expected:{}", got.len(), expected.len())
    }

    for (got, exp) in got.iter().zip(expected.iter()) {
        if exp.datum_type != got.datum_type {
            bail!("Failed to infer datum type: expected {:?}, got {:?}", exp, got);
        }
        if exp.shape != got.shape {
            bail!("Failed to infer shape: expected {:?}, got {:?}", exp, got);
        }
    }

    Ok(())
}
