//! Module-level and function-body validation.

use crate::error::ValidationError;
use crate::module::Module;

pub fn validate(_module: &Module) -> Result<(), ValidationError> {
    // Implemented in the next slice; the decoder is validated independently.
    Ok(())
}
