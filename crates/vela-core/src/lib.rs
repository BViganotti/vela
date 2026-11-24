//! Vela core: WebAssembly binary decoding, validation, and interpretation.
//!
//! The crate is deliberately dependency-free. Every parser is bounded by
//! explicit limits and every runtime access is checked.

pub mod binary;
pub mod error;
pub mod instr;
pub mod module;
pub mod runtime;
pub mod spec;
pub mod text;
pub mod types;
pub mod validate;

pub use error::{DecodeError, ValidationError};
pub use module::Module;
pub use runtime::trap::Trap;
pub use types::ValType;
