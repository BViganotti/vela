//! In-memory module representation shared by all backends.

use crate::instr::Instr;
use crate::types::{ExternType, FuncType, GlobalType, Limits, MemoryType, TableType, ValType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub desc: ImportDesc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportDesc {
    Func(u32),
    Table(TableType),
    Memory(MemoryType),
    Global(GlobalType),
}

impl ImportDesc {
    pub fn extern_type(&self) -> ExternType {
        match *self {
            ImportDesc::Func(i) => ExternType::Func(i),
            ImportDesc::Table(t) => ExternType::Table(t),
            ImportDesc::Memory(m) => ExternType::Memory(m),
            ImportDesc::Global(g) => ExternType::Global(g),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FuncBody {
    /// Declared locals, excluding parameters.
    pub locals: Vec<ValType>,
    /// Instructions including the terminating `end`.
    pub body: Vec<Instr>,
}

impl FuncBody {
    pub fn local_count(&self, params: usize) -> usize {
        params + self.locals.len()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Func {
    pub type_idx: u32,
    pub body: FuncBody,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Global {
    pub ty: GlobalType,
    pub init: Vec<Instr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub desc: ExportDesc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportDesc {
    Func(u32),
    Table(u32),
    Memory(u32),
    Global(u32),
}

impl ExportDesc {
    pub fn kind(self) -> &'static str {
        match self {
            ExportDesc::Func(_) => "func",
            ExportDesc::Table(_) => "table",
            ExportDesc::Memory(_) => "memory",
            ExportDesc::Global(_) => "global",
        }
    }

    pub fn index(self) -> u32 {
        match self {
            ExportDesc::Func(i)
            | ExportDesc::Table(i)
            | ExportDesc::Memory(i)
            | ExportDesc::Global(i) => i,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ElementMode {
    Passive,
    Active { table: u32, offset: Vec<Instr> },
    Declarative,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub mode: ElementMode,
    pub elem_type: ValType,
    /// Each item is a constant expression producing the reference.
    pub init: Vec<Vec<Instr>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DataMode {
    Passive,
    Active { memory: u32, offset: Vec<Instr> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    pub mode: DataMode,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Module {
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    pub funcs: Vec<Func>,
    pub tables: Vec<TableType>,
    pub memories: Vec<MemoryType>,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
    pub start: Option<u32>,
    pub elems: Vec<Element>,
    pub datas: Vec<Data>,
    /// Declared number of data segments (bulk memory encoding).
    pub data_count: Option<u32>,
    /// Custom section names, retained for diagnostics only.
    pub custom_sections: Vec<String>,
}

impl Module {
    /// Number of imported functions.
    pub fn imported_funcs(&self) -> usize {
        self.imports
            .iter()
            .filter(|i| matches!(i.desc, ImportDesc::Func(_)))
            .count()
    }

    pub fn imported_tables(&self) -> usize {
        self.imports
            .iter()
            .filter(|i| matches!(i.desc, ImportDesc::Table(_)))
            .count()
    }

    pub fn imported_memories(&self) -> usize {
        self.imports
            .iter()
            .filter(|i| matches!(i.desc, ImportDesc::Memory(_)))
            .count()
    }

    pub fn imported_globals(&self) -> usize {
        self.imports
            .iter()
            .filter(|i| matches!(i.desc, ImportDesc::Global(_)))
            .count()
    }

    /// Function type of every function in the module, imported first.
    pub fn func_types(&self) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::with_capacity(self.imports.len() + self.funcs.len());
        for imp in &self.imports {
            if let ImportDesc::Func(t) = imp.desc {
                out.push(t);
            }
        }
        for f in &self.funcs {
            out.push(f.type_idx);
        }
        out
    }

    /// Collect the type of every table, imported first.
    pub fn table_types(&self) -> Vec<TableType> {
        let mut out = Vec::new();
        for imp in &self.imports {
            if let ImportDesc::Table(t) = imp.desc {
                out.push(t);
            }
        }
        out.extend_from_slice(&self.tables);
        out
    }

    pub fn memory_types(&self) -> Vec<MemoryType> {
        let mut out = Vec::new();
        for imp in &self.imports {
            if let ImportDesc::Memory(m) = imp.desc {
                out.push(m);
            }
        }
        out.extend_from_slice(&self.memories);
        out
    }

    pub fn global_types(&self) -> Vec<GlobalType> {
        let mut out = Vec::new();
        for imp in &self.imports {
            if let ImportDesc::Global(g) = imp.desc {
                out.push(g);
            }
        }
        out.extend(self.globals.iter().map(|g| g.ty));
        out
    }

    pub fn limits_of_memory(&self, idx: u32) -> Option<Limits> {
        self.memory_types().get(idx as usize).map(|m| m.limits)
    }
}
