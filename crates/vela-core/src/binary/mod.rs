//! WebAssembly binary decoding and encoding.

pub mod decode;
pub mod reader;

pub use decode::{decode_module, decode_instr, encode_module};
pub use reader::{Reader, MAX_MODULE_BYTES};
