//! The store: the mutable state of all instantiated modules.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::module::Module;
use crate::runtime::global::GlobalInst;
use crate::runtime::instance::BodyInfo;
use crate::runtime::memory::MemoryInst;
use crate::runtime::table::TableInst;
use crate::runtime::trap::Trap;
use crate::runtime::value::Value;
use crate::types::{FuncType, GlobalType, Limits, MemoryType, TableType};

/// Address of an external value inside the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternVal {
    Func(u32),
    Table(u32),
    Memory(u32),
    Global(u32),
}

impl ExternVal {
    pub fn kind(&self) -> &'static str {
        match self {
            ExternVal::Func(_) => "func",
            ExternVal::Table(_) => "table",
            ExternVal::Memory(_) => "memory",
            ExternVal::Global(_) => "global",
        }
    }
}

pub type HostFn = dyn Fn(&mut HostEnv, &[Value]) -> Result<Vec<Value>, Trap>;

#[derive(Clone)]
pub struct HostFunc {
    pub ty: FuncType,
    pub name: String,
    pub func: Rc<HostFn>,
}

impl fmt::Debug for HostFunc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostFunc").field("name", &self.name).field("ty", &self.ty).finish()
    }
}

#[derive(Clone, Debug)]
pub enum FuncKind {
    /// A WebAssembly function: instance index and module-level function index.
    Wasm { instance: u32, func: u32 },
    Host(HostFunc),
}

#[derive(Clone, Debug)]
pub struct FuncInst {
    pub ty: FuncType,
    pub kind: FuncKind,
}

/// A fully instantiated module.
#[derive(Clone)]
pub struct Instance {
    pub module: Rc<Module>,
    /// Control-flow annotations for each defined function in `module.funcs`.
    pub codes: Vec<Rc<BodyInfo>>,
    pub func_addrs: Vec<u32>,
    pub table_addrs: Vec<u32>,
    pub mem_addrs: Vec<u32>,
    pub global_addrs: Vec<u32>,
    pub exports: BTreeMap<String, ExternVal>,
    /// Contents of element segments; `None` once dropped or declarative.
    pub elem_segments: Vec<Option<Rc<Vec<Value>>>>,
    /// Contents of data segments; `None` once dropped.
    pub data_segments: Vec<Option<Rc<Vec<u8>>>>,
}

impl fmt::Debug for Instance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Instance")
            .field("funcs", &self.func_addrs.len())
            .field("tables", &self.table_addrs.len())
            .field("memories", &self.mem_addrs.len())
            .field("globals", &self.global_addrs.len())
            .finish()
    }
}

/// Deterministic host environment. Recording and replay snapshot this value.
#[derive(Clone, Debug)]
pub struct HostEnv {
    clock_ms: u64,
    random_state: u64,
    calls: u64,
}

impl Default for HostEnv {
    fn default() -> Self {
        HostEnv { clock_ms: 0, random_state: 0x9e3779b97f4a7c15, calls: 0 }
    }
}

impl HostEnv {
    pub fn new(seed: u64) -> Self {
        HostEnv {
            clock_ms: 0,
            random_state: seed ^ 0x9e3779b97f4a7c15,
            calls: 0,
        }
    }

    /// Monotonic logical clock: one millisecond per observation.
    pub fn now_millis(&mut self) -> u64 {
        self.calls += 1;
        self.clock_ms = self.clock_ms.wrapping_add(1);
        self.clock_ms
    }

    pub fn random_u64(&mut self) -> u64 {
        self.calls += 1;
        // SplitMix64.
        self.random_state = self.random_state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.random_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    pub fn call_count(&self) -> u64 {
        self.calls
    }

    pub fn clock(&self) -> u64 {
        self.clock_ms
    }
}

/// The global store. Cloning a store produces a snapshot suitable for replay:
/// host functions, bodies and modules are shared through reference counting.
#[derive(Clone)]
pub struct Store {
    pub funcs: Vec<FuncInst>,
    pub tables: Vec<TableInst>,
    pub memories: Vec<MemoryInst>,
    pub globals: Vec<GlobalInst>,
    pub instances: Vec<Instance>,
    pub host: HostEnv,
    /// Hard cap for the initial size and growth of any memory.
    pub max_memory_bytes: usize,
    /// Hard cap for the initial size and growth of any table.
    pub max_table_entries: u32,
}

impl Default for Store {
    fn default() -> Self {
        Store {
            funcs: Vec::new(),
            tables: Vec::new(),
            memories: Vec::new(),
            globals: Vec::new(),
            instances: Vec::new(),
            host: HostEnv::default(),
            max_memory_bytes: crate::runtime::memory::DEFAULT_MAX_MEMORY_BYTES,
            max_table_entries: crate::runtime::table::DEFAULT_MAX_TABLE_ENTRIES,
        }
    }
}

impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Store")
            .field("funcs", &self.funcs.len())
            .field("tables", &self.tables.len())
            .field("memories", &self.memories.len())
            .field("globals", &self.globals.len())
            .field("instances", &self.instances.len())
            .finish()
    }
}

impl Store {
    pub fn new() -> Self {
        Store::default()
    }

    pub fn alloc_func(&mut self, inst: FuncInst) -> u32 {
        let addr = self.funcs.len() as u32;
        self.funcs.push(inst);
        addr
    }

    pub fn alloc_table(&mut self, table: TableInst) -> u32 {
        let addr = self.tables.len() as u32;
        self.tables.push(table);
        addr
    }

    pub fn alloc_memory(&mut self, memory: MemoryInst) -> u32 {
        let addr = self.memories.len() as u32;
        self.memories.push(memory);
        addr
    }

    pub fn alloc_global(&mut self, global: GlobalInst) -> u32 {
        let addr = self.globals.len() as u32;
        self.globals.push(global);
        addr
    }

    pub fn func(&self, addr: u32) -> Result<&FuncInst, Trap> {
        self.funcs.get(addr as usize).ok_or(Trap::UndefinedElement)
    }

    pub fn memory(&self, addr: u32) -> Result<&MemoryInst, Trap> {
        self.memories.get(addr as usize).ok_or(Trap::LimitExceeded("memory address"))
    }

    pub fn memory_mut(&mut self, addr: u32) -> Result<&mut MemoryInst, Trap> {
        self.memories.get_mut(addr as usize).ok_or(Trap::LimitExceeded("memory address"))
    }

    pub fn table(&self, addr: u32) -> Result<&TableInst, Trap> {
        self.tables.get(addr as usize).ok_or(Trap::LimitExceeded("table address"))
    }

    pub fn table_mut(&mut self, addr: u32) -> Result<&mut TableInst, Trap> {
        self.tables.get_mut(addr as usize).ok_or(Trap::LimitExceeded("table address"))
    }

    pub fn global(&self, addr: u32) -> Result<&GlobalInst, Trap> {
        self.globals.get(addr as usize).ok_or(Trap::LimitExceeded("global address"))
    }

    pub fn global_mut(&mut self, addr: u32) -> Result<&mut GlobalInst, Trap> {
        self.globals.get_mut(addr as usize).ok_or(Trap::LimitExceeded("global address"))
    }

    pub fn instance(&self, addr: u32) -> Result<&Instance, Trap> {
        self.instances.get(addr as usize).ok_or(Trap::LimitExceeded("instance address"))
    }

    /// Resolve an export by name from an instance.
    pub fn export(&self, instance: u32, name: &str) -> Option<ExternVal> {
        self.instances.get(instance as usize)?.exports.get(name).copied()
    }

    /// Hash mutable state: memories, tables, globals and host environment.
    pub fn state_hash(&self) -> u64 {
        let mut h = crate::runtime::value::Hasher::new();
        for m in &self.memories {
            h.write_u32(m.len() as u32);
            h.write_bytes(m.data());
        }
        for t in &self.tables {
            h.write_u32(t.len());
            for v in t.elements() {
                h.write_value(v);
            }
        }
        for g in &self.globals {
            h.write_value(&g.get());
        }
        for inst in &self.instances {
            for seg in &inst.data_segments {
                match seg {
                    Some(bytes) => {
                        h.write_u8(1);
                        h.write_bytes(bytes);
                    }
                    None => h.write_u8(0),
                }
            }
            for seg in &inst.elem_segments {
                match seg {
                    Some(vals) => {
                        h.write_u8(1);
                        for v in vals.iter() {
                            h.write_value(v);
                        }
                    }
                    None => h.write_u8(0),
                }
            }
        }
        h.write_u64(self.host.call_count());
        h.write_u64(self.host.clock());
        h.finish()
    }
}

/// Supplies imported external values during instantiation.
pub trait ImportResolver {
    fn resolve(&self, module: &str, name: &str) -> Option<ExternVal>;
}

/// A simple name-based registry used by tests and the Studio.
#[derive(Default)]
pub struct Registry {
    entries: HashMap<(String, String), ExternVal>,
    extra_funcs: Vec<(String, String, FuncType, Rc<HostFn>)>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, module: &str, name: &str, value: ExternVal) {
        self.entries.insert((module.to_string(), name.to_string()), value);
    }

    /// Register all exports of an instance under a module name.
    pub fn register_instance(&mut self, module_name: &str, instance: &Instance) {
        for (name, val) in &instance.exports {
            self.insert(module_name, name, *val);
        }
    }

    pub fn define_host(
        &mut self,
        module: &str,
        name: &str,
        ty: FuncType,
        f: Rc<HostFn>,
    ) {
        self.extra_funcs.push((module.to_string(), name.to_string(), ty, f));
    }

    /// Materialize host functions registered with `define_host` into a store.
    pub fn install_hosts(&mut self, store: &mut Store) {
        let extra = std::mem::take(&mut self.extra_funcs);
        for (module, name, ty, f) in extra {
            let addr = store.alloc_func(FuncInst {
                ty: ty.clone(),
                kind: FuncKind::Host(HostFunc { ty, name: name.clone(), func: f }),
            });
            self.insert(&module, &name, ExternVal::Func(addr));
        }
    }
}

impl ImportResolver for Registry {
    fn resolve(&self, module: &str, name: &str) -> Option<ExternVal> {
        self.entries.get(&(module.to_string(), name.to_string())).copied()
    }
}

/// A resolver that satisfies a fixed list of imports, used by the spec runner.
pub struct StaticResolver {
    pub entries: RefCell<Vec<(String, String, ExternVal)>>,
}

impl StaticResolver {
    pub fn new() -> Self {
        StaticResolver { entries: RefCell::new(Vec::new()) }
    }
    pub fn add(&self, module: &str, name: &str, v: ExternVal) {
        self.entries.borrow_mut().push((module.to_string(), name.to_string(), v));
    }
}

impl ImportResolver for StaticResolver {
    fn resolve(&self, module: &str, name: &str) -> Option<ExternVal> {
        self.entries
            .borrow()
            .iter()
            .rev()
            .find(|(m, n, _)| m == module && n == name)
            .map(|(_, _, v)| *v)
    }
}

/// Type compatibility for imports.
pub fn func_type_matches(expected: &FuncType, actual: &FuncType) -> bool {
    expected == actual
}

pub fn table_type_matches(expected: &TableType, actual: &TableType) -> bool {
    expected.elem == actual.elem && limits_match(expected.limits, actual.limits)
}

pub fn memory_type_matches(expected: &MemoryType, actual: &MemoryType) -> bool {
    limits_match(expected.limits, actual.limits)
}

pub fn global_type_matches(expected: &GlobalType, actual: &GlobalType) -> bool {
    expected == actual
}

/// Import matching: the provided limits must fit inside the expected ones.
pub fn limits_match(expected: Limits, actual: Limits) -> bool {
    if actual.min < expected.min {
        return false;
    }
    match expected.max {
        None => true,
        Some(exp_max) => match actual.max {
            Some(act_max) => act_max <= exp_max,
            None => false,
        },
    }
}
