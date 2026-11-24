//! Core WebAssembly types shared by decoder, validator, interpreter and text parser.

use std::fmt;

/// A WebAssembly value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
    FuncRef,
    ExternRef,
}

impl ValType {
    pub fn is_num(self) -> bool {
        matches!(
            self,
            ValType::I32 | ValType::I64 | ValType::F32 | ValType::F64
        )
    }

    pub fn is_ref(self) -> bool {
        matches!(self, ValType::FuncRef | ValType::ExternRef)
    }

    pub fn name(self) -> &'static str {
        match self {
            ValType::I32 => "i32",
            ValType::I64 => "i64",
            ValType::F32 => "f32",
            ValType::F64 => "f64",
            ValType::FuncRef => "funcref",
            ValType::ExternRef => "externref",
        }
    }

    pub fn from_name(name: &str) -> Option<ValType> {
        Some(match name {
            "i32" => ValType::I32,
            "i64" => ValType::I64,
            "f32" => ValType::F32,
            "f64" => ValType::F64,
            "funcref" => ValType::FuncRef,
            "externref" => ValType::ExternRef,
            _ => return None,
        })
    }

    /// Binary encoding byte for this value type.
    pub fn code(self) -> u8 {
        match self {
            ValType::I32 => 0x7F,
            ValType::I64 => 0x7E,
            ValType::F32 => 0x7D,
            ValType::F64 => 0x7C,
            ValType::FuncRef => 0x70,
            ValType::ExternRef => 0x6F,
        }
    }
}

impl fmt::Display for ValType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Function signature.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

impl FuncType {
    pub fn new(params: Vec<ValType>, results: Vec<ValType>) -> Self {
        FuncType { params, results }
    }
}

impl fmt::Display for FuncType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(func")?;
        for p in &self.params {
            write!(f, " (param {p})")?;
        }
        for r in &self.results {
            write!(f, " (result {r})")?;
        }
        f.write_str(")")
    }
}

/// Resizable limits used by memories and tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

impl Limits {
    pub fn new(min: u32, max: Option<u32>) -> Self {
        Limits { min, max }
    }

    pub fn is_valid(&self, hard_max: u32) -> bool {
        match self.max {
            Some(max) => self.min <= max && max <= hard_max,
            None => self.min <= hard_max,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TableType {
    pub elem: ValType,
    pub limits: Limits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MemoryType {
    pub limits: Limits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlobalType {
    pub val: ValType,
    pub mutable: bool,
}

impl GlobalType {
    pub fn const_of(val: ValType) -> Self {
        GlobalType {
            val,
            mutable: false,
        }
    }
}

/// External type of an import or export.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExternType {
    Func(u32),
    Table(TableType),
    Memory(MemoryType),
    Global(GlobalType),
}

impl ExternType {
    pub fn kind_name(&self) -> &'static str {
        match self {
            ExternType::Func(_) => "func",
            ExternType::Table(_) => "table",
            ExternType::Memory(_) => "memory",
            ExternType::Global(_) => "global",
        }
    }
}

/// Block signature: either empty, a single value, or a type index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockType {
    Empty,
    Value(ValType),
    TypeIdx(u32),
}

impl BlockType {
    pub fn result_count(&self) -> usize {
        match self {
            BlockType::Empty => 0,
            BlockType::Value(_) => 1,
            BlockType::TypeIdx(_) => usize::MAX, // resolved against the type section
        }
    }
}

/// A byte range into a data segment, used by active data segments.
pub const PAGE_SIZE: usize = 65536;
/// Maximum number of pages addressable by a 32-bit memory.
pub const MAX_PAGES: u32 = 65536;
/// Maximum table entries accepted by the engine.
pub const MAX_TABLE_ENTRIES: u32 = 10_000_000;
