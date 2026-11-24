//! Error types with stable, human-readable messages.

use std::fmt;

/// Binary decoding failure. Carries a byte offset where applicable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub offset: usize,
    pub message: String,
}

impl DecodeError {
    pub fn new(offset: usize, message: impl Into<String>) -> Self {
        DecodeError { offset, message: message.into() }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "decode error at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for DecodeError {}

/// Validation failure with the instruction offset and a stable code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub func: Option<u32>,
    pub offset: usize,
    pub code: &'static str,
    pub message: String,
}

impl ValidationError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        ValidationError { func: None, offset: 0, code, message: message.into() }
    }

    pub fn at(mut self, func: Option<u32>, offset: usize) -> Self {
        self.func = func;
        self.offset = offset;
        self
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.func {
            Some(func) => write!(
                f,
                "validation error in function {func} at instruction {:#x}: {} ({})",
                self.offset, self.message, self.code
            ),
            None => write!(f, "validation error: {} ({})", self.message, self.code),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Errors produced while parsing the textual format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextError {
    pub line: u32,
    pub col: u32,
    pub message: String,
}

impl TextError {
    pub fn new(line: u32, col: u32, message: impl Into<String>) -> Self {
        TextError { line, col, message: message.into() }
    }
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "text error at {}:{}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for TextError {}

pub type TextResult<T> = Result<T, TextError>;
