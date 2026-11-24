//! Typed runtime traps.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trap {
    Unreachable,
    MemoryOutOfBounds,
    TableOutOfBounds,
    IndirectCallTypeMismatch,
    UninitializedElement,
    UndefinedElement,
    IntegerDivideByZero,
    IntegerOverflow,
    InvalidConversionToInteger,
    StackOverflow,
    OutOfFuel,
    DataSegmentDropped,
    ElemSegmentDropped,
    UnalignedAtomic,
    Host(String),
    LimitExceeded(&'static str),
    TypeMismatch(&'static str),
}

impl Trap {
    pub fn code(&self) -> &'static str {
        match self {
            Trap::Unreachable => "unreachable",
            Trap::MemoryOutOfBounds => "out of bounds memory access",
            Trap::TableOutOfBounds => "out of bounds table access",
            Trap::IndirectCallTypeMismatch => "indirect call type mismatch",
            Trap::UninitializedElement => "uninitialized element",
            Trap::UndefinedElement => "undefined element",
            Trap::IntegerDivideByZero => "integer divide by zero",
            Trap::IntegerOverflow => "integer overflow",
            Trap::InvalidConversionToInteger => "invalid conversion to integer",
            Trap::StackOverflow => "call stack exhausted",
            Trap::OutOfFuel => "out of fuel",
            Trap::DataSegmentDropped => "data segment dropped",
            Trap::ElemSegmentDropped => "element segment dropped",
            Trap::UnalignedAtomic => "unaligned atomic",
            Trap::Host(_) => "host error",
            Trap::LimitExceeded(_) => "resource limit exceeded",
            Trap::TypeMismatch(_) => "type mismatch",
        }
    }
}

impl fmt::Display for Trap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Trap::Host(msg) => write!(f, "host error: {msg}"),
            Trap::LimitExceeded(msg) => write!(f, "resource limit exceeded: {msg}"),
            other => f.write_str(other.code()),
        }
    }
}

impl std::error::Error for Trap {}
