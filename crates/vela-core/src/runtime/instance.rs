//! Instantiation: linking, allocation, segment initialization and start.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use crate::error::ValidationError;
use crate::instr::Instr;
use crate::module::{DataMode, ElementMode, ExportDesc, ImportDesc, Module};
use crate::runtime::global::GlobalInst;
use crate::runtime::interp;
use crate::runtime::memory::MemoryInst;
use crate::runtime::store::{
    func_type_matches, global_type_matches, memory_type_matches, table_type_matches, ExternVal,
    FuncInst, FuncKind, ImportResolver, Instance, Store,
};
use crate::runtime::table::TableInst;
use crate::runtime::trap::Trap;
use crate::runtime::value::Value;
use crate::types::FuncType;
use crate::validate::validate_module;

/// Control-flow annotations for a function body, precomputed once.
#[derive(Clone, Debug, Default)]
pub struct BodyInfo {
    /// For each instruction index, the index of its matching `end`.
    pub end_of: Vec<usize>,
    /// For each `if`, the index of its `else` (if any).
    pub else_of: Vec<Option<usize>>,
}

impl BodyInfo {
    pub fn analyze(body: &[Instr]) -> BodyInfo {
        let mut end_of = vec![0usize; body.len()];
        let mut else_of = vec![None; body.len()];
        let mut stack: Vec<usize> = Vec::new();
        for (i, instr) in body.iter().enumerate() {
            match instr {
                Instr::Block(_) | Instr::Loop(_) | Instr::If(_) => stack.push(i),
                Instr::Else => {
                    if let Some(&open) = stack.last() {
                        else_of[open] = Some(i);
                    }
                }
                Instr::End => {
                    if let Some(open) = stack.pop() {
                        end_of[open] = i;
                    }
                    end_of[i] = i;
                }
                _ => {}
            }
        }
        BodyInfo { end_of, else_of }
    }
}

#[derive(Clone, Debug)]
pub enum InstantiateError {
    Validation(ValidationError),
    Trap(Trap),
    Import {
        module: String,
        name: String,
        message: String,
    },
    Limit(&'static str),
}

impl fmt::Display for InstantiateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstantiateError::Validation(e) => write!(f, "invalid module: {e}"),
            InstantiateError::Trap(t) => write!(f, "instantiation trapped: {t}"),
            InstantiateError::Import {
                module,
                name,
                message,
            } => {
                write!(f, "import {module}.{name}: {message}")
            }
            InstantiateError::Limit(m) => write!(f, "instantiation limit: {m}"),
        }
    }
}

impl std::error::Error for InstantiateError {}

impl From<Trap> for InstantiateError {
    fn from(t: Trap) -> Self {
        InstantiateError::Trap(t)
    }
}

impl From<ValidationError> for InstantiateError {
    fn from(e: ValidationError) -> Self {
        InstantiateError::Validation(e)
    }
}

fn import_error(module: &str, name: &str, message: impl Into<String>) -> InstantiateError {
    InstantiateError::Import {
        module: module.to_string(),
        name: name.to_string(),
        message: message.into(),
    }
}

fn eval_const(
    store: &Store,
    expr: &[Instr],
    func_addrs: &[u32],
    global_addrs: &[u32],
) -> Result<Value, Trap> {
    let mut stack: Vec<Value> = Vec::new();
    for instr in expr {
        match instr {
            Instr::End => break,
            Instr::I32Const(v) => stack.push(Value::I32(*v)),
            Instr::I64Const(v) => stack.push(Value::I64(*v)),
            Instr::F32Const(bits) => stack.push(Value::F32(f32::from_bits(*bits))),
            Instr::F64Const(bits) => stack.push(Value::F64(f64::from_bits(*bits))),
            Instr::RefNull(t) => stack.push(match t {
                crate::types::ValType::FuncRef => Value::FuncRef(None),
                _ => Value::ExternRef(None),
            }),
            Instr::RefFunc(i) => {
                let addr = *func_addrs.get(*i as usize).ok_or(Trap::UndefinedElement)?;
                stack.push(Value::FuncRef(Some(addr)));
            }
            Instr::GlobalGet(i) => {
                let addr = *global_addrs
                    .get(*i as usize)
                    .ok_or(Trap::UndefinedElement)?;
                stack.push(store.global(addr)?.get());
            }
            _ => return Err(Trap::TypeMismatch("constant expression")),
        }
    }
    if stack.len() != 1 {
        return Err(Trap::TypeMismatch("constant expression arity"));
    }
    Ok(stack.pop().unwrap())
}

/// Validate and instantiate a module. On failure all store allocations made
/// for this module are rolled back.
pub fn instantiate(
    store: &mut Store,
    module: Module,
    resolver: &dyn ImportResolver,
) -> Result<u32, InstantiateError> {
    validate_module(&module)?;
    let module = Rc::new(module);
    let instance_idx = store.instances.len() as u32;

    let saved = (
        store.funcs.len(),
        store.tables.len(),
        store.memories.len(),
        store.globals.len(),
        store.instances.len(),
    );

    let result = instantiate_inner(store, module, resolver, instance_idx);
    if result.is_err() {
        let (f, t, m, g, i) = saved;
        store.funcs.truncate(f);
        store.tables.truncate(t);
        store.memories.truncate(m);
        store.globals.truncate(g);
        store.instances.truncate(i);
    }
    result
}

fn instantiate_inner(
    store: &mut Store,
    module: Rc<Module>,
    resolver: &dyn ImportResolver,
    instance_idx: u32,
) -> Result<u32, InstantiateError> {
    let mut func_addrs: Vec<u32> = Vec::new();
    let mut table_addrs: Vec<u32> = Vec::new();
    let mut mem_addrs: Vec<u32> = Vec::new();
    let mut global_addrs: Vec<u32> = Vec::new();

    // Resolve imports and check their external types.
    for imp in &module.imports {
        let ext = resolver
            .resolve(&imp.module, &imp.name)
            .ok_or_else(|| import_error(&imp.module, &imp.name, "unknown import"))?;
        match (imp.desc, ext) {
            (ImportDesc::Func(type_idx), ExternVal::Func(addr)) => {
                let expected = &module.types[type_idx as usize];
                let actual = &store
                    .func(addr)
                    .map_err(|_| import_error(&imp.module, &imp.name, "unknown function address"))?
                    .ty;
                if !func_type_matches(expected, actual) {
                    return Err(import_error(
                        &imp.module,
                        &imp.name,
                        "function type mismatch",
                    ));
                }
                func_addrs.push(addr);
            }
            (ImportDesc::Table(expected), ExternVal::Table(addr)) => {
                let actual = store
                    .table(addr)
                    .map_err(|_| import_error(&imp.module, &imp.name, "unknown table address"))?;
                let actual_type = crate::types::TableType {
                    elem: actual.elem_type(),
                    limits: actual.limits(),
                };
                if !table_type_matches(&expected, &actual_type) {
                    return Err(import_error(&imp.module, &imp.name, "table type mismatch"));
                }
                table_addrs.push(addr);
            }
            (ImportDesc::Memory(expected), ExternVal::Memory(addr)) => {
                let actual = store
                    .memory(addr)
                    .map_err(|_| import_error(&imp.module, &imp.name, "unknown memory address"))?;
                let actual_type = crate::types::MemoryType {
                    limits: actual.limits(),
                };
                if !memory_type_matches(&expected, &actual_type) {
                    return Err(import_error(&imp.module, &imp.name, "memory type mismatch"));
                }
                mem_addrs.push(addr);
            }
            (ImportDesc::Global(expected), ExternVal::Global(addr)) => {
                let actual = store
                    .global(addr)
                    .map_err(|_| import_error(&imp.module, &imp.name, "unknown global address"))?;
                let actual_type = crate::types::GlobalType {
                    val: actual.ty(),
                    mutable: actual.is_mutable(),
                };
                if !global_type_matches(&expected, &actual_type) {
                    return Err(import_error(&imp.module, &imp.name, "global type mismatch"));
                }
                global_addrs.push(addr);
            }
            (desc, ext) => {
                return Err(import_error(
                    &imp.module,
                    &imp.name,
                    format!(
                        "kind mismatch: expected {}, found {}",
                        desc.extern_type().kind_name(),
                        ext.kind()
                    ),
                ))
            }
        }
    }

    // Allocate defined functions.
    for f in &module.funcs {
        let ty = module.types[f.type_idx as usize].clone();
        let addr = store.alloc_func(FuncInst {
            ty,
            kind: FuncKind::Wasm {
                instance: instance_idx,
                func: 0,
            },
        });
        // Patch the module-level function index.
        if let Some(FuncInst {
            kind: FuncKind::Wasm { func, .. },
            ..
        }) = store.funcs.get_mut(addr as usize)
        {
            *func = func_addrs.len() as u32;
        }
        func_addrs.push(addr);
    }

    // Allocate tables and memories.
    for t in &module.tables {
        let table = TableInst::try_new(t.elem, t.limits, store.max_table_entries)
            .ok_or(InstantiateError::Limit("table allocation"))?;
        let addr = store.alloc_table(table);
        table_addrs.push(addr);
    }
    for m in &module.memories {
        let memory = MemoryInst::try_new(m.limits, store.max_memory_bytes)
            .ok_or(InstantiateError::Limit("memory allocation"))?;
        let addr = store.alloc_memory(memory);
        mem_addrs.push(addr);
    }

    // Globals are initialized in order; only imported globals are readable.
    for g in &module.globals {
        let v = eval_const(store, &g.init, &func_addrs, &global_addrs)?;
        if v.ty() != g.ty.val {
            return Err(InstantiateError::Trap(Trap::TypeMismatch(
                "global initializer",
            )));
        }
        let addr = store.alloc_global(GlobalInst::new(g.ty, v));
        global_addrs.push(addr);
    }

    // Precompute control-flow metadata for every defined function.
    let codes: Vec<Rc<BodyInfo>> = module
        .funcs
        .iter()
        .map(|f| Rc::new(BodyInfo::analyze(&f.body.body)))
        .collect();

    let mut exports = BTreeMap::new();
    for e in &module.exports {
        let val = match e.desc {
            ExportDesc::Func(i) => ExternVal::Func(func_addrs[i as usize]),
            ExportDesc::Table(i) => ExternVal::Table(table_addrs[i as usize]),
            ExportDesc::Memory(i) => ExternVal::Memory(mem_addrs[i as usize]),
            ExportDesc::Global(i) => ExternVal::Global(global_addrs[i as usize]),
        };
        exports.insert(e.name.clone(), val);
    }

    // Element segments: evaluate items, apply active segments, retain passive.
    let mut elem_segments: Vec<Option<Rc<Vec<Value>>>> = Vec::with_capacity(module.elems.len());
    for e in &module.elems {
        let mut values = Vec::with_capacity(e.init.len());
        for item in &e.init {
            values.push(eval_const(store, item, &func_addrs, &global_addrs)?);
        }
        match &e.mode {
            ElementMode::Active { table, offset } => {
                let table_addr = *table_addrs
                    .get(*table as usize)
                    .ok_or(Trap::TableOutOfBounds)?;
                let off = eval_const(store, offset, &func_addrs, &global_addrs)?;
                let dst = off.as_i32().ok_or(Trap::TypeMismatch("element offset"))? as u32;
                let table_inst = store.table_mut(table_addr)?;
                table_inst.init(dst, &values, 0, values.len() as u32)?;
                elem_segments.push(None);
            }
            ElementMode::Passive => elem_segments.push(Some(Rc::new(values))),
            ElementMode::Declarative => elem_segments.push(None),
        }
    }

    // Data segments.
    let mut data_segments: Vec<Option<Rc<Vec<u8>>>> = Vec::with_capacity(module.datas.len());
    for d in &module.datas {
        match &d.mode {
            DataMode::Active { memory, offset } => {
                let mem_addr = *mem_addrs
                    .get(*memory as usize)
                    .ok_or(Trap::MemoryOutOfBounds)?;
                let off = eval_const(store, offset, &func_addrs, &global_addrs)?;
                let dst = off.as_i32().ok_or(Trap::TypeMismatch("data offset"))? as u32;
                let mem = store.memory_mut(mem_addr)?;
                mem.init(dst, &d.bytes, 0, d.bytes.len() as u32)?;
                data_segments.push(None);
            }
            DataMode::Passive => data_segments.push(Some(Rc::new(d.bytes.clone()))),
        }
    }

    let instance = Instance {
        module: module.clone(),
        codes,
        func_addrs: func_addrs.clone(),
        table_addrs,
        mem_addrs,
        global_addrs,
        exports,
        elem_segments,
        data_segments,
    };
    store.instances.push(instance);
    debug_assert_eq!(store.instances.len() as u32 - 1, instance_idx);

    if let Some(start) = module.start {
        let addr = func_addrs[start as usize];
        interp::invoke(store, addr, Vec::new(), interp::Limits::default())
            .map_err(InstantiateError::Trap)?;
    }

    Ok(instance_idx)
}

/// Allocate a host function and return its store address.
pub fn host_func(
    store: &mut Store,
    name: &str,
    ty: FuncType,
    f: Rc<crate::runtime::store::HostFn>,
) -> u32 {
    store.alloc_func(FuncInst {
        ty: ty.clone(),
        kind: FuncKind::Host(crate::runtime::store::HostFunc {
            ty,
            name: name.to_string(),
            func: f,
        }),
    })
}
