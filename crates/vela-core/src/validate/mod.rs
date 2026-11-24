//! Module validation.

use crate::error::ValidationError;
use crate::module::Module;

/// Validate a decoded module in full: types, indices, limits, bodies and
/// constant expressions.
pub fn validate_module(module: &Module) -> Result<(), ValidationError> {
    crate::validate::check::validate(module)
}

pub mod check;
