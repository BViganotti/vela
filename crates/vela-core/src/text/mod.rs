//! Text format support (WAT subset and .wast commands).

pub mod lexer;
pub mod print;
pub mod sexpr;
pub mod wat;

pub use print::module_to_wat;
pub use wat::parse_module as parse_wat;
