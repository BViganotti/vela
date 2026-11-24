//! `.wast` script parsing and execution against the Vela runtime.

pub mod report;
pub mod runner;

pub use report::{Failure, WastReport};
pub use runner::{run_script, run_wast};

/// A module source inside a `.wast` script.
#[derive(Clone, Debug, PartialEq)]
pub enum ModuleSource {
    Text(String),
    Binary(Vec<u8>),
    Quote(String),
}

/// A script action.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Invoke {
        module: Option<String>,
        field: String,
        args: Vec<Expected>,
    },
    Get {
        module: Option<String>,
        field: String,
    },
}

/// Literal expectation or argument.
#[derive(Clone, Debug, PartialEq)]
pub enum Expected {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
    F32Nan(NanKind),
    F64Nan(NanKind),
    RefNull(Option<crate::types::ValType>),
    RefExtern(u32),
    RefFunc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NanKind {
    Canonical,
    Arithmetic,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Module {
        name: Option<String>,
        source: ModuleSource,
    },
    Register {
        name: String,
        module: Option<String>,
    },
    Action(Action),
    AssertReturn {
        action: Action,
        expected: Vec<Expected>,
    },
    AssertTrap {
        action: Action,
        message: String,
    },
    AssertExhaustion {
        action: Action,
        message: String,
    },
    AssertInvalid {
        name: Option<String>,
        source: ModuleSource,
        message: String,
    },
    AssertMalformed {
        source: ModuleSource,
        message: String,
    },
    AssertUnlinkable {
        name: Option<String>,
        source: ModuleSource,
        message: String,
    },
    AssertNan {
        action: Action,
        kind: NanKind,
        bits: u32,
    },
}
