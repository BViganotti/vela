//! WebAssembly binary decoding and encoding.

pub mod decode;
pub mod reader;

pub use decode::{decode_instr, decode_module, encode_module, put_instr};
pub use reader::{Reader, MAX_MODULE_BYTES};
