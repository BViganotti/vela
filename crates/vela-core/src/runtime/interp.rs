//! The Vela interpreter: an explicit-frame stack machine.
//!
//! A step executes exactly one instruction. Calls push frames, returns pop
//! them, and all memory accesses are bounds checked. The machine never panics
//! on malformed inputs; every failure is a typed [`Trap`].

use std::rc::Rc;

use crate::instr::{Instr, LoadKind, MemArg, StoreKind};
use crate::module::Module;
use crate::runtime::instance::{BodyInfo, InstantiateError};
use crate::runtime::store::{ExternVal, FuncKind, Instance, Store};
use crate::runtime::trap::Trap;
use crate::runtime::value::Value;
use crate::types::{BlockType, FuncType, ValType};

/// Execution limits. All are hard bounds enforced by the interpreter.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_steps: u64,
    pub max_call_depth: usize,
    pub max_stack: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_steps: 10_000_000, max_call_depth: 1_024, max_stack: 1_000_000 }
    }
}

impl Limits {
    pub fn with_steps(max_steps: u64) -> Self {
        Limits { max_steps, ..Limits::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtrlKind {
    Func,
    Block,
    Loop,
    If,
    Else,
}

#[derive(Clone, Debug)]
pub struct Ctrl {
    pub kind: CtrlKind,
    pub start_ip: usize,
    pub end_ip: usize,
    pub stack_height: usize,
    /// Number of values transferred by a branch to this label.
    pub arity: usize,
    /// Number of values kept when the block ends normally.
    pub end_arity: usize,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub func_addr: u32,
    pub inst: u32,
    pub module: Rc<Module>,
    pub defined_idx: usize,
    pub info: Rc<BodyInfo>,
    pub locals: Vec<Value>,
    pub ip: usize,
    pub stack_base: usize,
    pub ctrl_base: usize,
    pub arity: usize,
}

#[derive(Clone, Debug, Default)]
pub struct ExecState {
    pub frames: Vec<Frame>,
    pub stack: Vec<Value>,
    pub ctrls: Vec<Ctrl>,
    pub steps: u64,
    pub finished: bool,
    pub results: Vec<Value>,
    pub trap: Option<Trap>,
}

impl ExecState {
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn call_depth(&self) -> usize {
        self.frames.len()
    }

    /// Current program counter: (function address, instruction index).
    pub fn pc(&self) -> Option<(u32, usize)> {
        self.frames.last().map(|f| (f.func_addr, f.ip))
    }

    pub fn current_func_name(&self, store: &Store) -> Option<String> {
        let f = self.frames.last()?;
        let inst = store.instances.get(f.inst as usize)?;
        Some(func_name(inst, f.defined_idx))
    }
}

/// Human-readable function label for traces and the Studio.
pub fn func_name(inst: &Instance, defined_idx: usize) -> String {
    // Use an export if one points at this function, else the index.
    for (name, val) in &inst.exports {
        if let ExternVal::Func(addr) = val {
            if inst.func_addrs.get(defined_idx + inst.module.imported_funcs()) == Some(addr) {
                return name.clone();
            }
        }
    }
    format!("func[{}]", defined_idx + inst.module.imported_funcs())
}

#[derive(Clone, Debug, PartialEq)]
pub enum StepResult {
    Running,
    Finished,
    Trapped(Trap),
}

pub struct Machine {
    pub limits: Limits,
}

impl Default for Machine {
    fn default() -> Self {
        Machine { limits: Limits::default() }
    }
}

impl Machine {
    pub fn new(limits: Limits) -> Self {
        Machine { limits }
    }

    /// Create an execution state for a function call. Arguments are checked
    /// against the function signature before any frame is pushed.
    pub fn start(
        &self,
        store: &mut Store,
        func_addr: u32,
        args: Vec<Value>,
    ) -> Result<ExecState, Trap> {
        let mut st = ExecState::default();
        enter_function(&mut st, store, func_addr, args, &self.limits)?;
        Ok(st)
    }

    /// Execute exactly one instruction.
    pub fn step(&self, store: &mut Store, st: &mut ExecState) -> StepResult {
        if st.finished {
            return StepResult::Finished;
        }
        if let Some(t) = &st.trap {
            return StepResult::Trapped(t.clone());
        }
        if st.steps >= self.limits.max_steps {
            let t = Trap::OutOfFuel;
            st.trap = Some(t.clone());
            return StepResult::Trapped(t);
        }
        let (ip, defined_idx, module, info, inst) = match st.frames.last() {
            None => {
                st.finished = true;
                return StepResult::Finished;
            }
            Some(f) => (f.ip, f.defined_idx, f.module.clone(), f.info.clone(), f.inst),
        };
        let body = &module.funcs[defined_idx].body.body;
        let instr = match body.get(ip) {
            Some(i) => i.clone(),
            None => {
                let t = Trap::TypeMismatch("instruction pointer out of range");
                st.trap = Some(t.clone());
                return StepResult::Trapped(t);
            }
        };
        if let Some(f) = st.frames.last_mut() {
            f.ip = ip + 1;
        }
        st.steps += 1;
        let result = self.execute(store, st, &instr, &module, inst, &info, ip);
        match result {
            Ok(()) => {
                if st.finished {
                    StepResult::Finished
                } else {
                    StepResult::Running
                }
            }
            Err(t) => {
                st.trap = Some(t.clone());
                StepResult::Trapped(t)
            }
        }
    }

    /// Run until return, trap, or the step limit.
    pub fn run(&self, store: &mut Store, st: &mut ExecState) -> Result<Vec<Value>, Trap> {
        loop {
            match self.step(store, st) {
                StepResult::Running => {}
                StepResult::Finished => return Ok(st.results.clone()),
                StepResult::Trapped(t) => return Err(t),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute(
        &self,
        store: &mut Store,
        st: &mut ExecState,
        instr: &Instr,
        module: &Module,
        inst: u32,
        info: &BodyInfo,
        ip: usize,
    ) -> Result<(), Trap> {
        use Instr::*;
        match instr {
            Unreachable => return Err(Trap::Unreachable),
            Nop => {}
            Block(bt) => {
                let (params, results) = block_sig(module, *bt)?;
                let height = block_height(st, params)?;
                st.ctrls.push(Ctrl {
                    kind: CtrlKind::Block,
                    start_ip: ip + 1,
                    end_ip: end_index(info, ip)?,
                    stack_height: height,
                    arity: results,
                    end_arity: results,
                });
            }
            Loop(bt) => {
                let (params, results) = block_sig(module, *bt)?;
                let height = block_height(st, params)?;
                st.ctrls.push(Ctrl {
                    kind: CtrlKind::Loop,
                    start_ip: ip + 1,
                    end_ip: end_index(info, ip)?,
                    stack_height: height,
                    arity: params,
                    end_arity: results,
                });
            }
            If(bt) => {
                let cond = pop_i32(st)?;
                let (params, results) = block_sig(module, *bt)?;
                let height = block_height(st, params)?;
                let end = end_index(info, ip)?;
                st.ctrls.push(Ctrl {
                    kind: CtrlKind::If,
                    start_ip: ip + 1,
                    end_ip: end,
                    stack_height: height,
                    arity: results,
                    end_arity: results,
                });
                if cond == 0 {
                    match info.else_of.get(ip).copied().flatten() {
                        Some(else_ip) => set_ip(st, else_ip + 1)?,
                        None => set_ip(st, end)?,
                    }
                }
            }
            Else => {
                // Reaching `else` means the then-branch finished: continue at
                // the matching `end`.
                let end = st
                    .ctrls
                    .last()
                    .ok_or(Trap::TypeMismatch("else without control frame"))?
                    .end_ip;
                set_ip(st, end)?;
            }
            End => {
                let kind = st
                    .ctrls
                    .last()
                    .ok_or(Trap::TypeMismatch("end without control frame"))?
                    .kind;
                if kind == CtrlKind::Func {
                    return_from_function(st)?;
                } else {
                    let ctrl = st.ctrls.pop().unwrap();
                    let arity = ctrl.end_arity;
                    let vals = split_off(st, arity)?;
                    st.stack.truncate(ctrl.stack_height);
                    st.stack.extend(vals);
                }
            }
            Br(depth) => branch(st, *depth)?,
            BrIf(depth) => {
                let cond = pop_i32(st)?;
                if cond != 0 {
                    branch(st, *depth)?;
                }
            }
            BrTable(table) => {
                let idx = pop_i32(st)?;
                let depth = if idx >= 0 {
                    table
                        .targets
                        .get(idx as usize)
                        .copied()
                        .unwrap_or(table.default)
                } else {
                    table.default
                };
                branch(st, depth)?;
            }
            Return => return_from_function(st)?,
            Call(func) => {
                let addr = *store
                    .instance(inst)?
                    .func_addrs
                    .get(*func as usize)
                    .ok_or(Trap::UndefinedElement)?;
                let ty = store.func(addr)?.ty.clone();
                let args = split_off(st, ty.params.len())?;
                enter_function(st, store, addr, args, &self.limits)?;
            }
            CallIndirect { type_idx, table_idx } => {
                let idx = pop_i32(st)? as u32;
                let table_addr = *store
                    .instance(inst)?
                    .table_addrs
                    .get(*table_idx as usize)
                    .ok_or(Trap::TableOutOfBounds)?;
                let table = store.table(table_addr)?;
                let slot = table.get(idx).map_err(|_| Trap::UndefinedElement)?;
                let func_addr = match slot {
                    Value::FuncRef(Some(a)) => a,
                    Value::FuncRef(None) => return Err(Trap::UninitializedElement),
                    _ => return Err(Trap::IndirectCallTypeMismatch),
                };
                let expected = module
                    .types
                    .get(*type_idx as usize)
                    .ok_or(Trap::TypeMismatch("call_indirect type"))?;
                if &store.func(func_addr)?.ty != expected {
                    return Err(Trap::IndirectCallTypeMismatch);
                }
                let args = split_off(st, expected.params.len())?;
                enter_function(st, store, func_addr, args, &self.limits)?;
            }
            Drop => {
                st.stack.pop().ok_or(Trap::TypeMismatch("drop"))?;
            }
            Select | SelectT(_) => {
                let cond = pop_i32(st)?;
                let b = st.stack.pop().ok_or(Trap::TypeMismatch("select"))?;
                let a = st.stack.pop().ok_or(Trap::TypeMismatch("select"))?;
                push(st, self.limits.max_stack, if cond != 0 { a } else { b })?;
            }
            LocalGet(i) => {
                let v = *frame(st)?.locals.get(*i as usize).ok_or(Trap::TypeMismatch("local"))?;
                push(st, self.limits.max_stack, v)?;
            }
            LocalSet(i) => {
                let v = st.stack.pop().ok_or(Trap::TypeMismatch("local.set"))?;
                let f = frame_mut(st)?;
                let slot = f.locals.get_mut(*i as usize).ok_or(Trap::TypeMismatch("local"))?;
                *slot = v;
            }
            LocalTee(i) => {
                let v = *st.stack.last().ok_or(Trap::TypeMismatch("local.tee"))?;
                let f = frame_mut(st)?;
                let slot = f.locals.get_mut(*i as usize).ok_or(Trap::TypeMismatch("local"))?;
                *slot = v;
            }
            GlobalGet(i) => {
                let addr = *store
                    .instance(inst)?
                    .global_addrs
                    .get(*i as usize)
                    .ok_or(Trap::UndefinedElement)?;
                let v = store.global(addr)?.get();
                push(st, self.limits.max_stack, v)?;
            }
            GlobalSet(i) => {
                let v = st.stack.pop().ok_or(Trap::TypeMismatch("global.set"))?;
                let addr = *store
                    .instance(inst)?
                    .global_addrs
                    .get(*i as usize)
                    .ok_or(Trap::UndefinedElement)?;
                store.global_mut(addr)?.set(v)?;
            }
            TableGet(t) => {
                let i = pop_i32(st)? as u32;
                let addr = *store
                    .instance(inst)?
                    .table_addrs
                    .get(*t as usize)
                    .ok_or(Trap::TableOutOfBounds)?;
                let v = store.table(addr)?.get(i)?;
                push(st, self.limits.max_stack, v)?;
            }
            TableSet(t) => {
                let v = st.stack.pop().ok_or(Trap::TypeMismatch("table.set"))?;
                let i = pop_i32(st)? as u32;
                let addr = *store
                    .instance(inst)?
                    .table_addrs
                    .get(*t as usize)
                    .ok_or(Trap::TableOutOfBounds)?;
                store.table_mut(addr)?.set(i, v)?;
            }
            TableGrow(t) => {
                let delta = pop_i32(st)?;
                let init = st.stack.pop().ok_or(Trap::TypeMismatch("table.grow"))?;
                let addr = *store
                    .instance(inst)?
                    .table_addrs
                    .get(*t as usize)
                    .ok_or(Trap::TableOutOfBounds)?;
                let old = store.table_mut(addr)?.grow(delta as u32, init);
                push(
                    st,
                    self.limits.max_stack,
                    Value::I32(old.map(|v| v as i32).unwrap_or(-1)),
                )?;
            }
            TableSize(t) => {
                let addr = *store
                    .instance(inst)?
                    .table_addrs
                    .get(*t as usize)
                    .ok_or(Trap::TableOutOfBounds)?;
                let n = store.table(addr)?.len();
                push(st, self.limits.max_stack, Value::I32(n as i32))?;
            }
            TableFill(t) => {
                let n = pop_i32(st)?;
                let val = st.stack.pop().ok_or(Trap::TypeMismatch("table.fill"))?;
                let i = pop_i32(st)?;
                let addr = *store
                    .instance(inst)?
                    .table_addrs
                    .get(*t as usize)
                    .ok_or(Trap::TableOutOfBounds)?;
                store.table_mut(addr)?.fill(i as u32, val, n as u32)?;
            }
            TableInit { elem, table } => {
                let n = pop_i32(st)?;
                let s = pop_i32(st)?;
                let d = pop_i32(st)?;
                let (table_addr, seg) = {
                    let inst_ref = store.instance(inst)?;
                    let table_addr = *inst_ref
                        .table_addrs
                        .get(*table as usize)
                        .ok_or(Trap::TableOutOfBounds)?;
                    let seg = inst_ref
                        .elem_segments
                        .get(*elem as usize)
                        .ok_or(Trap::ElemSegmentDropped)?
                        .as_ref()
                        .ok_or(Trap::ElemSegmentDropped)?
                        .clone();
                    (table_addr, seg)
                };
                let table_inst = store.table_mut(table_addr)?;
                table_inst.init(d as u32, seg.as_slice(), s as u32, n as u32)?;
            }
            TableCopy { dst, src } => {
                let n = pop_i32(st)?;
                let s = pop_i32(st)?;
                let d = pop_i32(st)?;
                let inst_ref = store.instance(inst)?;
                let dst_addr = *inst_ref
                    .table_addrs
                    .get(*dst as usize)
                    .ok_or(Trap::TableOutOfBounds)?;
                let src_addr = *inst_ref
                    .table_addrs
                    .get(*src as usize)
                    .ok_or(Trap::TableOutOfBounds)?;
                if dst_addr == src_addr {
                    store.table_mut(dst_addr)?.copy(d as u32, s as u32, n as u32)?;
                } else {
                    let vals: Vec<Value> = {
                        let src_table = store.table(src_addr)?;
                        let s0 = s as u32 as u64;
                        let n0 = n as u32 as u64;
                        if s0 + n0 > src_table.len() as u64 {
                            return Err(Trap::TableOutOfBounds);
                        }
                        (0..n0)
                            .map(|k| src_table.get((s0 + k) as u32).unwrap_or(Value::FuncRef(None)))
                            .collect()
                    };
                    store
                        .table_mut(dst_addr)?
                        .init(d as u32, &vals, 0, n as u32)?;
                }
            }
            ElemDrop(e) => {
                let inst_ref = store.instance(inst)?;
                if let Some(slot) = inst_ref.elem_segments.get(*e as usize) {
                    if slot.is_none() {
                        return Err(Trap::ElemSegmentDropped);
                    }
                } else {
                    return Err(Trap::ElemSegmentDropped);
                }
                let instance = &mut store.instances[inst as usize];
                instance.elem_segments[*e as usize] = None;
            }
            MemorySize(m) => {
                let addr = *store
                    .instance(inst)?
                    .mem_addrs
                    .get(*m as usize)
                    .ok_or(Trap::MemoryOutOfBounds)?;
                let pages = store.memory(addr)?.size_pages();
                push(st, self.limits.max_stack, Value::I32(pages as i32))?;
            }
            MemoryGrow(m) => {
                let delta = pop_i32(st)?;
                let addr = *store
                    .instance(inst)?
                    .mem_addrs
                    .get(*m as usize)
                    .ok_or(Trap::MemoryOutOfBounds)?;
                let old = store.memory_mut(addr)?.grow(delta as u32);
                push(
                    st,
                    self.limits.max_stack,
                    Value::I32(old.map(|v| v as i32).unwrap_or(-1)),
                )?;
            }
            MemoryInit { data, mem } => {
                let n = pop_i32(st)?;
                let s = pop_i32(st)?;
                let d = pop_i32(st)?;
                let (mem_addr, seg) = {
                    let inst_ref = store.instance(inst)?;
                    let mem_addr = *inst_ref
                        .mem_addrs
                        .get(*mem as usize)
                        .ok_or(Trap::MemoryOutOfBounds)?;
                    let seg = inst_ref
                        .data_segments
                        .get(*data as usize)
                        .ok_or(Trap::DataSegmentDropped)?
                        .as_ref()
                        .ok_or(Trap::DataSegmentDropped)?
                        .clone();
                    (mem_addr, seg)
                };
                let mem_inst = store.memory_mut(mem_addr)?;
                mem_inst.init(d as u32, seg.as_slice(), s as u32, n as u32)?;
            }
            DataDrop(d) => {
                let inst_ref = store.instance(inst)?;
                match inst_ref.data_segments.get(*d as usize) {
                    Some(Some(_)) => {}
                    _ => return Err(Trap::DataSegmentDropped),
                }
                store.instances[inst as usize].data_segments[*d as usize] = None;
            }
            MemoryCopy { dst, src } => {
                let n = pop_i32(st)?;
                let s = pop_i32(st)?;
                let d = pop_i32(st)?;
                let inst_ref = store.instance(inst)?;
                let dst_addr = *inst_ref
                    .mem_addrs
                    .get(*dst as usize)
                    .ok_or(Trap::MemoryOutOfBounds)?;
                let src_addr = *inst_ref
                    .mem_addrs
                    .get(*src as usize)
                    .ok_or(Trap::MemoryOutOfBounds)?;
                if dst_addr == src_addr {
                    store.memory_mut(dst_addr)?.copy(d as u32, s as u32, n as u32)?;
                } else {
                    let bytes: Vec<u8> = {
                        let m = store.memory(src_addr)?;
                        let s0 = s as u32 as u64;
                        let n0 = n as u32 as u64;
                        if s0 + n0 > m.len() as u64 {
                            return Err(Trap::MemoryOutOfBounds);
                        }
                        m.data()[s0 as usize..(s0 + n0) as usize].to_vec()
                    };
                    store.memory_mut(dst_addr)?.init(d as u32, &bytes, 0, n as u32)?;
                }
            }
            MemoryFill(m) => {
                let n = pop_i32(st)?;
                let val = pop_i32(st)? as u8;
                let d = pop_i32(st)?;
                let addr = *store
                    .instance(inst)?
                    .mem_addrs
                    .get(*m as usize)
                    .ok_or(Trap::MemoryOutOfBounds)?;
                store.memory_mut(addr)?.fill(d as u32, val, n as u32)?;
            }
            Load { kind, mem, arg } => {
                let addr = pop_i32(st)? as u32;
                let mem_addr = *store
                    .instance(inst)?
                    .mem_addrs
                    .get(*mem as usize)
                    .ok_or(Trap::MemoryOutOfBounds)?;
                let v = load(store.memory(mem_addr)?, *kind, addr, *arg)?;
                push(st, self.limits.max_stack, v)?;
            }
            Store { kind, mem, arg } => {
                let v = st.stack.pop().ok_or(Trap::TypeMismatch("store value"))?;
                let addr = pop_i32(st)? as u32;
                let mem_addr = *store
                    .instance(inst)?
                    .mem_addrs
                    .get(*mem as usize)
                    .ok_or(Trap::MemoryOutOfBounds)?;
                store_mem(store.memory_mut(mem_addr)?, *kind, addr, *arg, v)?;
            }
            I32Const(v) => push(st, self.limits.max_stack, Value::I32(*v))?,
            I64Const(v) => push(st, self.limits.max_stack, Value::I64(*v))?,
            F32Const(bits) => push(st, self.limits.max_stack, Value::F32(f32::from_bits(*bits)))?,
            F64Const(bits) => push(st, self.limits.max_stack, Value::F64(f64::from_bits(*bits)))?,
            RefNull(t) => push(
                st,
                self.limits.max_stack,
                match t {
                    ValType::FuncRef => Value::FuncRef(None),
                    _ => Value::ExternRef(None),
                },
            )?,
            RefIsNull => {
                let v = st.stack.pop().ok_or(Trap::TypeMismatch("ref.is_null"))?;
                push(
                    st,
                    self.limits.max_stack,
                    Value::I32(if v.is_null_ref() { 1 } else { 0 }),
                )?;
            }
            RefFunc(f) => {
                let addr = *store
                    .instance(inst)?
                    .func_addrs
                    .get(*f as usize)
                    .ok_or(Trap::UndefinedElement)?;
                push(st, self.limits.max_stack, Value::FuncRef(Some(addr)))?;
            }
            other => {
                let v = eval_numeric(other, st)?;
                push(st, self.limits.max_stack, v)?;
            }
        }
        Ok(())
    }
}

fn frame(st: &ExecState) -> Result<&Frame, Trap> {
    st.frames.last().ok_or(Trap::TypeMismatch("no frame"))
}

fn frame_mut(st: &mut ExecState) -> Result<&mut Frame, Trap> {
    st.frames.last_mut().ok_or(Trap::TypeMismatch("no frame"))
}

fn set_ip(st: &mut ExecState, ip: usize) -> Result<(), Trap> {
    frame_mut(st)?.ip = ip;
    Ok(())
}

fn push(st: &mut ExecState, max: usize, v: Value) -> Result<(), Trap> {
    if st.stack.len() >= max {
        return Err(Trap::LimitExceeded("value stack"));
    }
    st.stack.push(v);
    Ok(())
}

fn pop(st: &mut ExecState) -> Result<Value, Trap> {
    st.stack.pop().ok_or(Trap::TypeMismatch("stack underflow"))
}

fn pop_i32(st: &mut ExecState) -> Result<i32, Trap> {
    match pop(st)? {
        Value::I32(v) => Ok(v),
        _ => Err(Trap::TypeMismatch("expected i32")),
    }
}

fn pop_i64(st: &mut ExecState) -> Result<i64, Trap> {
    match pop(st)? {
        Value::I64(v) => Ok(v),
        _ => Err(Trap::TypeMismatch("expected i64")),
    }
}

fn pop_f32(st: &mut ExecState) -> Result<f32, Trap> {
    match pop(st)? {
        Value::F32(v) => Ok(v),
        _ => Err(Trap::TypeMismatch("expected f32")),
    }
}

fn pop_f64(st: &mut ExecState) -> Result<f64, Trap> {
    match pop(st)? {
        Value::F64(v) => Ok(v),
        _ => Err(Trap::TypeMismatch("expected f64")),
    }
}

fn split_off(st: &mut ExecState, n: usize) -> Result<Vec<Value>, Trap> {
    if st.stack.len() < n {
        return Err(Trap::TypeMismatch("stack underflow (call arguments)"));
    }
    Ok(st.stack.split_off(st.stack.len() - n))
}

fn block_sig(module: &Module, bt: BlockType) -> Result<(usize, usize), Trap> {
    match bt {
        BlockType::Empty => Ok((0, 0)),
        BlockType::Value(_) => Ok((0, 1)),
        BlockType::TypeIdx(i) => {
            let ty = module.types.get(i as usize).ok_or(Trap::TypeMismatch("block type"))?;
            Ok((ty.params.len(), ty.results.len()))
        }
    }
}

fn block_height(st: &ExecState, params: usize) -> Result<usize, Trap> {
    st.stack
        .len()
        .checked_sub(params)
        .ok_or(Trap::TypeMismatch("block parameters"))
}

fn end_index(info: &BodyInfo, ip: usize) -> Result<usize, Trap> {
    info.end_of.get(ip).copied().ok_or(Trap::TypeMismatch("block end"))
}

fn enter_function(
    st: &mut ExecState,
    store: &mut Store,
    func_addr: u32,
    args: Vec<Value>,
    limits: &Limits,
) -> Result<(), Trap> {
    if st.frames.len() >= limits.max_call_depth {
        return Err(Trap::StackOverflow);
    }
    let func = store.func(func_addr)?.clone();
    if args.len() != func.ty.params.len() {
        return Err(Trap::TypeMismatch("argument count"));
    }
    for (arg, expected) in args.iter().zip(func.ty.params.iter()) {
        if arg.ty() != *expected {
            return Err(Trap::TypeMismatch("argument type"));
        }
    }
    match func.kind {
        FuncKind::Host(h) => {
            let results = (h.func)(&mut store.host, &args)?;
            if results.len() != func.ty.results.len() {
                return Err(Trap::TypeMismatch("host result count"));
            }
            for (v, expected) in results.iter().zip(func.ty.results.iter()) {
                if v.ty() != *expected {
                    return Err(Trap::TypeMismatch("host result type"));
                }
            }
            for v in results {
                push(st, limits.max_stack, v)?;
            }
        }
        FuncKind::Wasm { instance, func: func_index } => {
            let inst = store
                .instance(instance)
                .map_err(|_| Trap::TypeMismatch("function instance"))?;
            let imported = inst.module.imported_funcs();
            let defined_idx = (func_index as usize)
                .checked_sub(imported)
                .ok_or(Trap::TypeMismatch("imported function body"))?;
            let module = inst.module.clone();
            let info = inst
                .codes
                .get(defined_idx)
                .ok_or(Trap::TypeMismatch("missing function metadata"))?
                .clone();
            let body = module
                .funcs
                .get(defined_idx)
                .ok_or(Trap::TypeMismatch("missing function body"))?;
            if body.body.body.is_empty() {
                return Err(Trap::TypeMismatch("empty function body"));
            }
            let local_count = func.ty.params.len() + body.body.locals.len();
            if local_count > limits.max_stack {
                return Err(Trap::LimitExceeded("local count"));
            }
            let mut locals = args;
            for ty in &body.body.locals {
                locals.push(Value::default_for(*ty));
            }
            let stack_base = st.stack.len();
            let ctrl_base = st.ctrls.len();
            st.ctrls.push(Ctrl {
                kind: CtrlKind::Func,
                start_ip: 0,
                end_ip: body.body.body.len() - 1,
                stack_height: stack_base,
                arity: func.ty.results.len(),
                end_arity: func.ty.results.len(),
            });
            st.frames.push(Frame {
                func_addr,
                inst: instance,
                module,
                defined_idx,
                info,
                locals,
                ip: 0,
                stack_base,
                ctrl_base,
                arity: func.ty.results.len(),
            });
        }
    }
    Ok(())
}

fn return_from_function(st: &mut ExecState) -> Result<(), Trap> {
    let frame = st.frames.pop().ok_or(Trap::TypeMismatch("return without frame"))?;
    let n = frame.arity;
    let vals = split_off(st, n)?;
    st.stack.truncate(frame.stack_base);
    st.stack.extend(vals.iter().copied());
    st.ctrls.truncate(frame.ctrl_base);
    if st.frames.is_empty() {
        st.finished = true;
        st.results = vals;
    }
    Ok(())
}

fn branch(st: &mut ExecState, depth: u32) -> Result<(), Trap> {
    let idx = st
        .ctrls
        .len()
        .checked_sub(1 + depth as usize)
        .ok_or(Trap::TypeMismatch("unknown label"))?;
    let ctrl = st.ctrls[idx].clone();
    if ctrl.kind == CtrlKind::Func {
        return return_from_function(st);
    }
    let vals = split_off(st, ctrl.arity)?;
    st.stack.truncate(ctrl.stack_height);
    st.stack.extend(vals);
    if ctrl.kind == CtrlKind::Loop {
        st.ctrls.truncate(idx + 1);
        set_ip(st, ctrl.start_ip)?;
    } else {
        st.ctrls.truncate(idx);
        set_ip(st, ctrl.end_ip + 1)?;
    }
    Ok(())
}

fn load(mem: &crate::runtime::memory::MemoryInst, kind: LoadKind, addr: u32, arg: MemArg) -> Result<Value, Trap> {
    let mut buf = [0u8; 8];
    let width = kind.width() as usize;
    mem.read(addr, arg.offset, &mut buf[..width])?;
    Ok(match kind {
        LoadKind::I32 => Value::I32(i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])),
        LoadKind::I64 => Value::I64(i64::from_le_bytes(buf)),
        LoadKind::F32 => Value::F32(f32::from_bits(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]))),
        LoadKind::F64 => Value::F64(f64::from_bits(u64::from_le_bytes(buf))),
        LoadKind::I32x8S => Value::I32(buf[0] as i8 as i32),
        LoadKind::I32x8U => Value::I32(buf[0] as i32),
        LoadKind::I32x16S => Value::I32(i16::from_le_bytes([buf[0], buf[1]]) as i32),
        LoadKind::I32x16U => Value::I32(u16::from_le_bytes([buf[0], buf[1]]) as i32),
        LoadKind::I64x8S => Value::I64(buf[0] as i8 as i64),
        LoadKind::I64x8U => Value::I64(buf[0] as i64),
        LoadKind::I64x16S => Value::I64(i16::from_le_bytes([buf[0], buf[1]]) as i64),
        LoadKind::I64x16U => Value::I64(u16::from_le_bytes([buf[0], buf[1]]) as i64),
        LoadKind::I64x32S => Value::I64(i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as i64),
        LoadKind::I64x32U => Value::I64(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as i64),
    })
}

fn store_mem(
    mem: &mut crate::runtime::memory::MemoryInst,
    kind: StoreKind,
    addr: u32,
    arg: MemArg,
    v: Value,
) -> Result<(), Trap> {
    let bytes = match (kind, v) {
        (StoreKind::I32, Value::I32(x)) => x.to_le_bytes().to_vec(),
        (StoreKind::I64, Value::I64(x)) => x.to_le_bytes().to_vec(),
        (StoreKind::F32, Value::F32(x)) => x.to_bits().to_le_bytes().to_vec(),
        (StoreKind::F64, Value::F64(x)) => x.to_bits().to_le_bytes().to_vec(),
        (StoreKind::I32x8, Value::I32(x)) => vec![x as u8],
        (StoreKind::I32x16, Value::I32(x)) => (x as u16).to_le_bytes().to_vec(),
        (StoreKind::I64x8, Value::I64(x)) => vec![x as u8],
        (StoreKind::I64x16, Value::I64(x)) => (x as u16).to_le_bytes().to_vec(),
        (StoreKind::I64x32, Value::I64(x)) => (x as u32).to_le_bytes().to_vec(),
        _ => return Err(Trap::TypeMismatch("store value type")),
    };
    mem.write(addr, arg.offset, &bytes)
}

// --- Numeric operations -----------------------------------------------------

fn f32_min(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        return f32::from_bits(0x7fc0_0000);
    }
    if a == b {
        return if a.is_sign_negative() || b.is_sign_negative() { -0.0 } else { a };
    }
    if a < b { a } else { b }
}

fn f32_max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        return f32::from_bits(0x7fc0_0000);
    }
    if a == b {
        return if a.is_sign_positive() || b.is_sign_positive() { 0.0 } else { a };
    }
    if a > b { a } else { b }
}

fn f64_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::from_bits(0x7ff8_0000_0000_0000);
    }
    if a == b {
        return if a.is_sign_negative() || b.is_sign_negative() { -0.0 } else { a };
    }
    if a < b { a } else { b }
}

fn f64_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::from_bits(0x7ff8_0000_0000_0000);
    }
    if a == b {
        return if a.is_sign_positive() || b.is_sign_positive() { 0.0 } else { a };
    }
    if a > b { a } else { b }
}

fn trunc_f32_to_i32(x: f32, signed: bool) -> Result<i32, Trap> {
    if x.is_nan() {
        return Err(Trap::InvalidConversionToInteger);
    }
    let t = (x as f64).trunc();
    if signed {
        if !(-2147483648.0..2147483648.0).contains(&t) {
            return Err(Trap::IntegerOverflow);
        }
        Ok(t as i32)
    } else {
        if !(0.0..4294967296.0).contains(&t) {
            if t < 0.0 {
                return Err(Trap::IntegerOverflow);
            }
            return Err(Trap::IntegerOverflow);
        }
        Ok(t as u32 as i32)
    }
}

fn trunc_f64_to_i32(x: f64, signed: bool) -> Result<i32, Trap> {
    if x.is_nan() {
        return Err(Trap::InvalidConversionToInteger);
    }
    let t = x.trunc();
    if signed {
        if !(-2147483648.0..2147483648.0).contains(&t) {
            return Err(Trap::IntegerOverflow);
        }
        Ok(t as i32)
    } else {
        if !(0.0..4294967296.0).contains(&t) {
            return Err(Trap::IntegerOverflow);
        }
        Ok(t as u32 as i32)
    }
}

fn trunc_f32_to_i64(x: f32, signed: bool) -> Result<i64, Trap> {
    if x.is_nan() {
        return Err(Trap::InvalidConversionToInteger);
    }
    let t = (x as f64).trunc();
    if signed {
        if !(-9223372036854775808.0..9223372036854775808.0).contains(&t) {
            return Err(Trap::IntegerOverflow);
        }
        Ok(t as i64)
    } else {
        if !(0.0..18446744073709551616.0).contains(&t) {
            return Err(Trap::IntegerOverflow);
        }
        Ok(t as u64 as i64)
    }
}

fn trunc_f64_to_i64(x: f64, signed: bool) -> Result<i64, Trap> {
    if x.is_nan() {
        return Err(Trap::InvalidConversionToInteger);
    }
    let t = x.trunc();
    if signed {
        if !(-9223372036854775808.0..9223372036854775808.0).contains(&t) {
            return Err(Trap::IntegerOverflow);
        }
        Ok(t as i64)
    } else {
        if !(0.0..18446744073709551616.0).contains(&t) {
            return Err(Trap::IntegerOverflow);
        }
        Ok(t as u64 as i64)
    }
}

fn sat_i32(x: f64, signed: bool) -> i32 {
    if x.is_nan() {
        0
    } else if signed {
        if x <= -2147483648.0 {
            i32::MIN
        } else if x >= 2147483648.0 {
            i32::MAX
        } else {
            x.trunc() as i32
        }
    } else if x <= 0.0 {
        0
    } else if x >= 4294967296.0 {
        u32::MAX as i32
    } else {
        x.trunc() as u32 as i32
    }
}

fn sat_i64(x: f64, signed: bool) -> i64 {
    if x.is_nan() {
        0
    } else if signed {
        if x <= -9223372036854775808.0 {
            i64::MIN
        } else if x >= 9223372036854775808.0 {
            i64::MAX
        } else {
            x.trunc() as i64
        }
    } else if x <= 0.0 {
        0
    } else if x >= 18446744073709551616.0 {
        u64::MAX as i64
    } else {
        x.trunc() as u64 as i64
    }
}

fn eval_numeric(instr: &Instr, st: &mut ExecState) -> Result<Value, Trap> {
    use Instr::*;
    Ok(match instr {
        I32Eqz => Value::I32((pop_i32(st)? == 0) as i32),
        I64Eqz => Value::I32((pop_i64(st)? == 0) as i32),
        I32Eq => bin_i32(st, |a, b| (a == b) as i32)?,
        I32Ne => bin_i32(st, |a, b| (a != b) as i32)?,
        I32LtS => bin_i32(st, |a, b| (a < b) as i32)?,
        I32LtU => bin_i32(st, |a, b| ((a as u32) < (b as u32)) as i32)?,
        I32GtS => bin_i32(st, |a, b| (a > b) as i32)?,
        I32GtU => bin_i32(st, |a, b| ((a as u32) > (b as u32)) as i32)?,
        I32LeS => bin_i32(st, |a, b| (a <= b) as i32)?,
        I32LeU => bin_i32(st, |a, b| ((a as u32) <= (b as u32)) as i32)?,
        I32GeS => bin_i32(st, |a, b| (a >= b) as i32)?,
        I32GeU => bin_i32(st, |a, b| ((a as u32) >= (b as u32)) as i32)?,
        I64Eq => bin_i64_cmp(st, |a, b| a == b)?,
        I64Ne => bin_i64_cmp(st, |a, b| a != b)?,
        I64LtS => bin_i64_cmp(st, |a, b| a < b)?,
        I64LtU => bin_i64_cmp(st, |a, b| (a as u64) < (b as u64))?,
        I64GtS => bin_i64_cmp(st, |a, b| a > b)?,
        I64GtU => bin_i64_cmp(st, |a, b| (a as u64) > (b as u64))?,
        I64LeS => bin_i64_cmp(st, |a, b| a <= b)?,
        I64LeU => bin_i64_cmp(st, |a, b| (a as u64) <= (b as u64))?,
        I64GeS => bin_i64_cmp(st, |a, b| a >= b)?,
        I64GeU => bin_i64_cmp(st, |a, b| (a as u64) >= (b as u64))?,
        F32Eq => bin_f32_cmp(st, |a, b| a == b)?,
        F32Ne => bin_f32_cmp(st, |a, b| a != b)?,
        F32Lt => bin_f32_cmp(st, |a, b| a < b)?,
        F32Gt => bin_f32_cmp(st, |a, b| a > b)?,
        F32Le => bin_f32_cmp(st, |a, b| a <= b)?,
        F32Ge => bin_f32_cmp(st, |a, b| a >= b)?,
        F64Eq => bin_f64_cmp(st, |a, b| a == b)?,
        F64Ne => bin_f64_cmp(st, |a, b| a != b)?,
        F64Lt => bin_f64_cmp(st, |a, b| a < b)?,
        F64Gt => bin_f64_cmp(st, |a, b| a > b)?,
        F64Le => bin_f64_cmp(st, |a, b| a <= b)?,
        F64Ge => bin_f64_cmp(st, |a, b| a >= b)?,
        I32Clz => Value::I32(pop_i32(st)?.leading_zeros() as i32),
        I32Ctz => Value::I32(pop_i32(st)?.trailing_zeros() as i32),
        I32Popcnt => Value::I32(pop_i32(st)?.count_ones() as i32),
        I32Add => bin_i32(st, i32::wrapping_add)?,
        I32Sub => bin_i32(st, i32::wrapping_sub)?,
        I32Mul => bin_i32(st, i32::wrapping_mul)?,
        I32DivS => {
            let b = pop_i32(st)?;
            let a = pop_i32(st)?;
            if b == 0 {
                return Err(Trap::IntegerDivideByZero);
            }
            if a == i32::MIN && b == -1 {
                return Err(Trap::IntegerOverflow);
            }
            Value::I32(a / b)
        }
        I32DivU => {
            let b = pop_i32(st)? as u32;
            let a = pop_i32(st)? as u32;
            if b == 0 {
                return Err(Trap::IntegerDivideByZero);
            }
            Value::I32((a / b) as i32)
        }
        I32RemS => {
            let b = pop_i32(st)?;
            let a = pop_i32(st)?;
            if b == 0 {
                return Err(Trap::IntegerDivideByZero);
            }
            Value::I32(if b == -1 { 0 } else { a % b })
        }
        I32RemU => {
            let b = pop_i32(st)? as u32;
            let a = pop_i32(st)? as u32;
            if b == 0 {
                return Err(Trap::IntegerDivideByZero);
            }
            Value::I32((a % b) as i32)
        }
        I32And => bin_i32(st, |a, b| a & b)?,
        I32Or => bin_i32(st, |a, b| a | b)?,
        I32Xor => bin_i32(st, |a, b| a ^ b)?,
        I32Shl => bin_i32(st, |a, b| a.wrapping_shl(b as u32 & 31))?,
        I32ShrS => bin_i32(st, |a, b| a >> (b as u32 & 31))?,
        I32ShrU => bin_i32(st, |a, b| ((a as u32) >> (b as u32 & 31)) as i32)?,
        I32Rotl => bin_i32(st, |a, b| a.rotate_left(b as u32 & 31))?,
        I32Rotr => bin_i32(st, |a, b| a.rotate_right(b as u32 & 31))?,
        I64Clz => Value::I64(pop_i64(st)?.leading_zeros() as i64),
        I64Ctz => Value::I64(pop_i64(st)?.trailing_zeros() as i64),
        I64Popcnt => Value::I64(pop_i64(st)?.count_ones() as i64),
        I64Add => bin_i64(st, i64::wrapping_add)?,
        I64Sub => bin_i64(st, i64::wrapping_sub)?,
        I64Mul => bin_i64(st, i64::wrapping_mul)?,
        I64DivS => {
            let b = pop_i64(st)?;
            let a = pop_i64(st)?;
            if b == 0 {
                return Err(Trap::IntegerDivideByZero);
            }
            if a == i64::MIN && b == -1 {
                return Err(Trap::IntegerOverflow);
            }
            Value::I64(a / b)
        }
        I64DivU => {
            let b = pop_i64(st)? as u64;
            let a = pop_i64(st)? as u64;
            if b == 0 {
                return Err(Trap::IntegerDivideByZero);
            }
            Value::I64((a / b) as i64)
        }
        I64RemS => {
            let b = pop_i64(st)?;
            let a = pop_i64(st)?;
            if b == 0 {
                return Err(Trap::IntegerDivideByZero);
            }
            Value::I64(if b == -1 { 0 } else { a % b })
        }
        I64RemU => {
            let b = pop_i64(st)? as u64;
            let a = pop_i64(st)? as u64;
            if b == 0 {
                return Err(Trap::IntegerDivideByZero);
            }
            Value::I64((a % b) as i64)
        }
        I64And => bin_i64(st, |a, b| a & b)?,
        I64Or => bin_i64(st, |a, b| a | b)?,
        I64Xor => bin_i64(st, |a, b| a ^ b)?,
        I64Shl => bin_i64(st, |a, b| a.wrapping_shl(b as u32 & 63))?,
        I64ShrS => bin_i64(st, |a, b| a >> (b as u32 & 63))?,
        I64ShrU => bin_i64(st, |a, b| ((a as u64) >> (b as u32 & 63)) as i64)?,
        I64Rotl => bin_i64(st, |a, b| a.rotate_left(b as u32 & 63))?,
        I64Rotr => bin_i64(st, |a, b| a.rotate_right(b as u32 & 63))?,
        F32Abs => Value::F32(pop_f32(st)?.abs()),
        F32Neg => Value::F32(-pop_f32(st)?),
        F32Ceil => Value::F32(pop_f32(st)?.ceil()),
        F32Floor => Value::F32(pop_f32(st)?.floor()),
        F32Trunc => Value::F32(pop_f32(st)?.trunc()),
        F32Nearest => Value::F32(pop_f32(st)?.round_ties_even()),
        F32Sqrt => Value::F32(pop_f32(st)?.sqrt()),
        F32Add => bin_f32(st, |a, b| a + b)?,
        F32Sub => bin_f32(st, |a, b| a - b)?,
        F32Mul => bin_f32(st, |a, b| a * b)?,
        F32Div => bin_f32(st, |a, b| a / b)?,
        F32Min => bin_f32(st, f32_min)?,
        F32Max => bin_f32(st, f32_max)?,
        F32Copysign => bin_f32(st, |a, b| a.copysign(b))?,
        F64Abs => Value::F64(pop_f64(st)?.abs()),
        F64Neg => Value::F64(-pop_f64(st)?),
        F64Ceil => Value::F64(pop_f64(st)?.ceil()),
        F64Floor => Value::F64(pop_f64(st)?.floor()),
        F64Trunc => Value::F64(pop_f64(st)?.trunc()),
        F64Nearest => Value::F64(pop_f64(st)?.round_ties_even()),
        F64Sqrt => Value::F64(pop_f64(st)?.sqrt()),
        F64Add => bin_f64(st, |a, b| a + b)?,
        F64Sub => bin_f64(st, |a, b| a - b)?,
        F64Mul => bin_f64(st, |a, b| a * b)?,
        F64Div => bin_f64(st, |a, b| a / b)?,
        F64Min => bin_f64(st, f64_min)?,
        F64Max => bin_f64(st, f64_max)?,
        F64Copysign => bin_f64(st, |a, b| a.copysign(b))?,
        I32WrapI64 => Value::I32(pop_i64(st)? as i32),
        I32TruncF32S => Value::I32(trunc_f32_to_i32(pop_f32(st)?, true)?),
        I32TruncF32U => Value::I32(trunc_f32_to_i32(pop_f32(st)?, false)?),
        I32TruncF64S => Value::I32(trunc_f64_to_i32(pop_f64(st)?, true)?),
        I32TruncF64U => Value::I32(trunc_f64_to_i32(pop_f64(st)?, false)?),
        I64ExtendI32S => Value::I64(pop_i32(st)? as i64),
        I64ExtendI32U => Value::I64(pop_i32(st)? as u32 as u64 as i64),
        I64TruncF32S => Value::I64(trunc_f32_to_i64(pop_f32(st)?, true)?),
        I64TruncF32U => Value::I64(trunc_f32_to_i64(pop_f32(st)?, false)?),
        I64TruncF64S => Value::I64(trunc_f64_to_i64(pop_f64(st)?, true)?),
        I64TruncF64U => Value::I64(trunc_f64_to_i64(pop_f64(st)?, false)?),
        F32ConvertI32S => Value::F32(pop_i32(st)? as f32),
        F32ConvertI32U => Value::F32(pop_i32(st)? as u32 as f32),
        F32ConvertI64S => Value::F32(pop_i64(st)? as f32),
        F32ConvertI64U => Value::F32(pop_i64(st)? as u64 as f32),
        F32DemoteF64 => Value::F32(pop_f64(st)? as f32),
        F64ConvertI32S => Value::F64(pop_i32(st)? as f64),
        F64ConvertI32U => Value::F64(pop_i32(st)? as u32 as f64),
        F64ConvertI64S => Value::F64(pop_i64(st)? as f64),
        F64ConvertI64U => Value::F64(pop_i64(st)? as u64 as f64),
        F64PromoteF32 => Value::F64(pop_f32(st)? as f64),
        I32ReinterpretF32 => Value::I32(pop_f32(st)?.to_bits() as i32),
        I64ReinterpretF64 => Value::I64(pop_f64(st)?.to_bits() as i64),
        F32ReinterpretI32 => Value::F32(f32::from_bits(pop_i32(st)? as u32)),
        F64ReinterpretI64 => Value::F64(f64::from_bits(pop_i64(st)? as u64)),
        I32Extend8S => Value::I32(pop_i32(st)? as i8 as i32),
        I32Extend16S => Value::I32(pop_i32(st)? as i16 as i32),
        I64Extend8S => Value::I64(pop_i64(st)? as i8 as i64),
        I64Extend16S => Value::I64(pop_i64(st)? as i16 as i64),
        I64Extend32S => Value::I64(pop_i64(st)? as i32 as i64),
        I32TruncSatF32S => Value::I32(sat_i32(pop_f32(st)? as f64, true)),
        I32TruncSatF32U => Value::I32(sat_i32(pop_f32(st)? as f64, false)),
        I32TruncSatF64S => Value::I32(sat_i32(pop_f64(st)?, true)),
        I32TruncSatF64U => Value::I32(sat_i32(pop_f64(st)?, false)),
        I64TruncSatF32S => Value::I64(sat_i64(pop_f32(st)? as f64, true)),
        I64TruncSatF32U => Value::I64(sat_i64(pop_f32(st)? as f64, false)),
        I64TruncSatF64S => Value::I64(sat_i64(pop_f64(st)?, true)),
        I64TruncSatF64U => Value::I64(sat_i64(pop_f64(st)?, false)),
        _ => return Err(Trap::TypeMismatch("not a numeric instruction")),
    })
}

fn bin_i32(st: &mut ExecState, f: impl Fn(i32, i32) -> i32) -> Result<Value, Trap> {
    let b = pop_i32(st)?;
    let a = pop_i32(st)?;
    Ok(Value::I32(f(a, b)))
}

fn bin_i64(st: &mut ExecState, f: impl Fn(i64, i64) -> i64) -> Result<Value, Trap> {
    let b = pop_i64(st)?;
    let a = pop_i64(st)?;
    Ok(Value::I64(f(a, b)))
}

fn bin_f32(st: &mut ExecState, f: impl Fn(f32, f32) -> f32) -> Result<Value, Trap> {
    let b = pop_f32(st)?;
    let a = pop_f32(st)?;
    Ok(Value::F32(f(a, b)))
}

fn bin_f64(st: &mut ExecState, f: impl Fn(f64, f64) -> f64) -> Result<Value, Trap> {
    let b = pop_f64(st)?;
    let a = pop_f64(st)?;
    Ok(Value::F64(f(a, b)))
}

fn bin_i64_cmp(st: &mut ExecState, f: impl Fn(i64, i64) -> bool) -> Result<Value, Trap> {
    let b = pop_i64(st)?;
    let a = pop_i64(st)?;
    Ok(Value::I32(f(a, b) as i32))
}

fn bin_f32_cmp(st: &mut ExecState, f: impl Fn(f32, f32) -> bool) -> Result<Value, Trap> {
    let b = pop_f32(st)?;
    let a = pop_f32(st)?;
    Ok(Value::I32(f(a, b) as i32))
}

fn bin_f64_cmp(st: &mut ExecState, f: impl Fn(f64, f64) -> bool) -> Result<Value, Trap> {
    let b = pop_f64(st)?;
    let a = pop_f64(st)?;
    Ok(Value::I32(f(a, b) as i32))
}

/// Invoke a function to completion with explicit limits.
pub fn invoke(
    store: &mut Store,
    func_addr: u32,
    args: Vec<Value>,
    limits: Limits,
) -> Result<Vec<Value>, Trap> {
    let machine = Machine::new(limits);
    let mut st = machine.start(store, func_addr, args)?;
    machine.run(store, &mut st)
}

/// Invoke an exported function by name.
pub fn invoke_export(
    store: &mut Store,
    instance: u32,
    name: &str,
    args: Vec<Value>,
    limits: Limits,
) -> Result<Vec<Value>, InstantiateError> {
    let ext = store
        .export(instance, name)
        .ok_or_else(|| InstantiateError::Import {
            module: "?".into(),
            name: name.into(),
            message: "unknown export".into(),
        })?;
    match ext {
        ExternVal::Func(addr) => invoke(store, addr, args, limits).map_err(InstantiateError::Trap),
        _ => Err(InstantiateError::Import {
            module: "?".into(),
            name: name.into(),
            message: "export is not a function".into(),
        }),
    }
}

/// Type of an exported function (used by the spec runner and Studio).
pub fn export_func_type(store: &Store, instance: u32, name: &str) -> Option<FuncType> {
    match store.export(instance, name)? {
        ExternVal::Func(addr) => Some(store.func(addr).ok()?.ty.clone()),
        _ => None,
    }
}
