//! Debug sessions: forward stepping, checkpoints, breakpoints and time travel.

use std::fmt;

use vela_core::error::{TextError, ValidationError};
use vela_core::instr::{Instr, LoadKind, StoreKind};
use vela_core::module::Module;
use vela_core::runtime::{
    instantiate, ExecState, ExternVal, FuncKind, Hasher, InstantiateError, Limits, Machine, StepResult,
    Store, Trap, Value,
};
use vela_core::text::parse_wat;
use vela_core::types::ValType;

use crate::record::{Trace, TraceEvent};
use crate::rewind::CheckpointLog;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Breakpoint {
    Function { func_addr: u32 },
    Instruction { func_addr: u32, offset: usize },
    MemoryAddress(u32),
    HostCall,
}

#[derive(Clone, Debug)]
pub struct SessionStatus {
    pub step: u64,
    pub finished: bool,
    pub trap: Option<Trap>,
    pub pc: Option<(u32, usize)>,
    pub call_depth: usize,
    pub stack_height: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunStop {
    Finished,
    Trapped(Trap),
    Breakpoint(Breakpoint),
    StepLimit,
}

impl RunStop {
    pub fn name(&self) -> &'static str {
        match self {
            RunStop::Finished => "finished",
            RunStop::Trapped(_) => "trapped",
            RunStop::Breakpoint(_) => "breakpoint",
            RunStop::StepLimit => "step-limit",
        }
    }
}

#[derive(Clone, Debug)]
pub enum SessionError {
    Text(TextError),
    Validation(ValidationError),
    Instantiate(InstantiateError),
    MissingExport(String),
    Trap(Trap),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::Text(e) => write!(f, "{e}"),
            SessionError::Validation(e) => write!(f, "{e}"),
            SessionError::Instantiate(e) => write!(f, "{e}"),
            SessionError::MissingExport(name) => write!(f, "missing function export {name:?}"),
            SessionError::Trap(t) => write!(f, "{t}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<TextError> for SessionError {
    fn from(e: TextError) -> Self {
        SessionError::Text(e)
    }
}

impl From<InstantiateError> for SessionError {
    fn from(e: InstantiateError) -> Self {
        SessionError::Instantiate(e)
    }
}

impl From<Trap> for SessionError {
    fn from(t: Trap) -> Self {
        SessionError::Trap(t)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    Trap(Trap),
    OutOfRange { target: u64, current: u64 },
    NoCheckpoint(u64),
    NotRunning,
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayError::Trap(t) => write!(f, "replay trapped: {t}"),
            ReplayError::OutOfRange { target, current } => {
                write!(f, "cannot rewind to step {target} from step {current}")
            }
            ReplayError::NoCheckpoint(step) => write!(f, "no checkpoint at or before step {step}"),
            ReplayError::NotRunning => write!(f, "session is not running"),
        }
    }
}

impl std::error::Error for ReplayError {}

pub struct Session {
    pub store: Store,
    pub exec: ExecState,
    pub machine: Machine,
    pub trace: Trace,
    pub checkpoints: CheckpointLog,
    pub breakpoints: Vec<Breakpoint>,
    suppress_break_at: Option<u64>,
}

impl Session {
    pub fn new(
        mut store: Store,
        func_addr: u32,
        args: Vec<Value>,
        limits: Limits,
        checkpoint_interval: u64,
    ) -> Result<Self, SessionError> {
        let machine = Machine::new(limits);
        let exec = machine.start(&mut store, func_addr, args)?;
        let mut s = Session {
            store,
            exec,
            machine,
            trace: Trace::new(),
            checkpoints: CheckpointLog::new(checkpoint_interval),
            breakpoints: Vec::new(),
            suppress_break_at: None,
        };
        s.checkpoints.capture(0, &s.store, &s.exec, 0);
        Ok(s)
    }

    pub fn from_instance(
        store: Store,
        instance: u32,
        export: &str,
        args: Vec<Value>,
        limits: Limits,
        checkpoint_interval: u64,
    ) -> Result<Self, SessionError> {
        let addr = match store.export(instance, export) {
            Some(ExternVal::Func(a)) => a,
            _ => return Err(SessionError::MissingExport(export.to_string())),
        };
        Session::new(store, addr, args, limits, checkpoint_interval)
    }

    /// Build a session from WAT text with no imports.
    pub fn from_wat(
        src: &str,
        export: &str,
        args: Vec<Value>,
        limits: Limits,
    ) -> Result<Self, SessionError> {
        let module: Module = parse_wat(src)?;
        let mut store = Store::new();
        let instance = instantiate(&mut store, module, &vela_core::runtime::Registry::new())?;
        Session::from_instance(store, instance, export, args, limits, 64)
    }

    pub fn step_forward(&mut self) -> StepResult {
        if self.exec.finished {
            return StepResult::Finished;
        }
        if let Some(t) = &self.exec.trap {
            return StepResult::Trapped(t.clone());
        }
        if self.exec.steps >= self.machine.limits.max_steps {
            let t = Trap::OutOfFuel;
            self.exec.trap = Some(t.clone());
            return StepResult::Trapped(t);
        }
        let event = self.capture_event();
        let result = self.machine.step(&mut self.store, &mut self.exec);
        if let Some(event) = event {
            self.trace.push(event);
        }
        self.checkpoints
            .maybe_capture(self.exec.steps, &self.store, &self.exec, self.trace.len());
        result
    }

    fn capture_event(&self) -> Option<TraceEvent> {
        let frame = self.exec.frames.last()?;
        let body = &frame.module.funcs[frame.defined_idx].body.body;
        let op = body.get(frame.ip).map(|i| i.name()).unwrap_or("<invalid>");
        let mut h = Hasher::new();
        for v in &self.exec.stack {
            h.write_value(v);
        }
        Some(TraceEvent {
            step: self.exec.steps,
            func_addr: frame.func_addr,
            offset: frame.ip,
            op,
            stack_height: self.exec.stack.len(),
            call_depth: self.exec.frames.len(),
            stack_hash: h.finish(),
        })
    }

    pub fn run(&mut self) -> RunStop {
        loop {
            if self.exec.finished {
                return RunStop::Finished;
            }
            if let Some(t) = &self.exec.trap {
                return RunStop::Trapped(t.clone());
            }
            if self.exec.steps >= self.machine.limits.max_steps {
                return RunStop::StepLimit;
            }
            if self.should_stop() {
                return match self.hit_breakpoint() {
                    Some(bp) => RunStop::Breakpoint(bp),
                    None => RunStop::StepLimit,
                };
            }
            match self.step_forward() {
                StepResult::Running => {}
                StepResult::Finished => return RunStop::Finished,
                StepResult::Trapped(t) => return RunStop::Trapped(t),
            }
        }
    }

    /// Continue after a breakpoint: skip the breakpoint at the current pc once.
    pub fn resume(&mut self) -> RunStop {
        if self.exec.steps >= self.machine.limits.max_steps {
            return RunStop::StepLimit;
        }
        self.suppress_break_at = Some(self.exec.steps);
        self.run()
    }

    fn should_stop(&mut self) -> bool {
        if self.suppress_break_at == Some(self.exec.steps) {
            self.suppress_break_at = None;
            return false;
        }
        self.hit_breakpoint().is_some()
    }

    /// Run without respecting breakpoints.
    pub fn run_to_completion(&mut self) -> RunStop {
        loop {
            if self.exec.finished {
                return RunStop::Finished;
            }
            if let Some(t) = &self.exec.trap {
                return RunStop::Trapped(t.clone());
            }
            if self.exec.steps >= self.machine.limits.max_steps {
                return RunStop::StepLimit;
            }
            if let StepResult::Trapped(t) = self.step_forward() {
                return RunStop::Trapped(t);
            }
        }
    }

    pub fn step_back(&mut self) -> Result<(), ReplayError> {
        if self.exec.steps == 0 {
            return Err(ReplayError::NotRunning);
        }
        self.rewind_to(self.exec.steps - 1)
    }

    /// Restore the state at `target` by rewinding to a checkpoint and replaying.
    pub fn rewind_to(&mut self, target: u64) -> Result<(), ReplayError> {
        let current = self.exec.steps;
        if target > current {
            return Err(ReplayError::OutOfRange { target, current });
        }
        if target == current {
            return Ok(());
        }
        let checkpoint = self
            .checkpoints
            .nearest(target)
            .cloned()
            .ok_or(ReplayError::NoCheckpoint(target))?;
        self.store = checkpoint.store;
        self.exec = checkpoint.exec;
        self.trace.truncate(checkpoint.trace_len);
        self.checkpoints.truncate_after(target);
        while self.exec.steps < target {
            if self.exec.finished {
                return Err(ReplayError::Trap(Trap::Host(
                    "replay finished before target step".to_string(),
                )));
            }
            match self.step_forward() {
                StepResult::Running => {}
                StepResult::Finished => {
                    if self.exec.steps < target {
                        return Err(ReplayError::Trap(Trap::Host(
                            "replay finished before target step".to_string(),
                        )));
                    }
                }
                StepResult::Trapped(t) => return Err(ReplayError::Trap(t)),
            }
        }
        Ok(())
    }

    pub fn status(&self) -> SessionStatus {
        SessionStatus {
            step: self.exec.steps,
            finished: self.exec.finished,
            trap: self.exec.trap.clone(),
            pc: self.exec.pc(),
            call_depth: self.exec.frames.len(),
            stack_height: self.exec.stack.len(),
        }
    }

    pub fn trace_hash(&self) -> u64 {
        self.trace.hash()
    }

    pub fn state_hash(&self) -> u64 {
        let mut h = Hasher::new();
        h.write_u64(self.exec.steps);
        h.write_u8(self.exec.finished as u8);
        match &self.exec.trap {
            Some(t) => {
                h.write_u8(1);
                h.write_bytes(t.code().as_bytes());
            }
            None => h.write_u8(0),
        }
        for frame in &self.exec.frames {
            h.write_u32(frame.func_addr);
            h.write_u32(frame.inst);
            h.write_u64(frame.ip as u64);
            h.write_u64(frame.stack_base as u64);
            h.write_u64(frame.ctrl_base as u64);
            for v in &frame.locals {
                h.write_value(v);
            }
        }
        for v in &self.exec.stack {
            h.write_value(v);
        }
        for c in &self.exec.ctrls {
            h.write_u8(match c.kind {
                vela_core::runtime::interp::CtrlKind::Func => 0,
                vela_core::runtime::interp::CtrlKind::Block => 1,
                vela_core::runtime::interp::CtrlKind::Loop => 2,
                vela_core::runtime::interp::CtrlKind::If => 3,
                vela_core::runtime::interp::CtrlKind::Else => 4,
            });
            h.write_u64(c.start_ip as u64);
            h.write_u64(c.end_ip as u64);
            h.write_u64(c.stack_height as u64);
            h.write_u64(c.arity as u64);
            h.write_u64(c.end_arity as u64);
        }
        for v in &self.exec.results {
            h.write_value(v);
        }
        h.write_u64(self.store.state_hash());
        h.finish()
    }

    pub fn add_breakpoint(&mut self, bp: Breakpoint) {
        if !self.breakpoints.contains(&bp) {
            self.breakpoints.push(bp);
        }
    }

    pub fn remove_breakpoint(&mut self, bp: &Breakpoint) {
        self.breakpoints.retain(|b| b != bp);
    }

    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    pub fn hit_breakpoint(&self) -> Option<Breakpoint> {
        if self.breakpoints.is_empty() {
            return None;
        }
        let frame = self.exec.frames.last()?;
        let (func_addr, offset) = (frame.func_addr, frame.ip);
        let instr = frame
            .module
            .funcs
            .get(frame.defined_idx)
            .and_then(|f| f.body.body.get(offset));
        for bp in &self.breakpoints {
            let hit = match bp {
                Breakpoint::Function { func_addr: a } => *a == func_addr,
                Breakpoint::Instruction { func_addr: a, offset: o } => {
                    *a == func_addr && *o == offset
                }
                Breakpoint::MemoryAddress(address) => {
                    instr.map(|i| memory_access_hits(i, *address, &self.exec)).unwrap_or(false)
                }
                Breakpoint::HostCall => instr
                    .map(|i| host_call_targets(i, &self.store, &self.exec, frame.inst))
                    .unwrap_or(false),
            };
            if hit {
                return Some(bp.clone());
            }
        }
        None
    }

    /// Current instruction and the function it belongs to.
    pub fn current_instruction(&self) -> Option<(u32, usize, Instr)> {
        let frame = self.exec.frames.last()?;
        let body = &frame.module.funcs.get(frame.defined_idx)?.body.body;
        Some((frame.func_addr, frame.ip, body.get(frame.ip)?.clone()))
    }

    /// Resolve a function address to a readable name.
    pub fn func_name(&self, func_addr: u32) -> String {
        for (i, inst) in self.store.instances.iter().enumerate() {
            for (name, ext) in &inst.exports {
                if let ExternVal::Func(a) = ext {
                    if *a == func_addr {
                        return name.clone();
                    }
                }
            }
            if inst.func_addrs.contains(&func_addr) {
                if let Some(idx) = inst.func_addrs.iter().position(|a| *a == func_addr) {
                    return format!("func[{idx}]@{}", i);
                }
            }
        }
        format!("func@{func_addr}")
    }

    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    /// Snapshot of visible locals for the current frame (Studio inspection).
    pub fn locals(&self) -> Vec<(u32, Value, ValType)> {
        let Some(frame) = self.exec.frames.last() else {
            return Vec::new();
        };
        let params = frame
            .module
            .funcs
            .get(frame.defined_idx)
            .map(|f| frame.module.types[f.type_idx as usize].params.len())
            .unwrap_or(0);
        frame
            .locals
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let ty = if i < params {
                    frame.module.types[frame.module.funcs[frame.defined_idx].type_idx as usize]
                        .params[i]
                } else {
                    frame.module.funcs[frame.defined_idx]
                        .body
                        .locals
                        .get(i - params)
                        .copied()
                        .unwrap_or(v.ty())
                };
                (i as u32, *v, ty)
            })
            .collect()
    }
}

fn memory_access_hits(instr: &Instr, bp: u32, exec: &ExecState) -> bool {
    let (addr, width) = match instr {
        Instr::Load { kind, arg: _, .. } => {
            let Some(Value::I32(a)) = exec.stack.last() else {
                return false;
            };
            (*a as u32, kind.width())
        }
        Instr::Store { kind, arg: _, .. } => {
            if exec.stack.len() < 2 {
                return false;
            }
            match exec.stack[exec.stack.len() - 2] {
                Value::I32(a) => (a as u32, kind.width()),
                _ => return false,
            }
        }
        _ => return false,
    };
    let effective = addr as u64 + match instr {
        Instr::Load { arg, .. } | Instr::Store { arg, .. } => arg.offset as u64,
        _ => 0,
    };
    let target = bp as u64;
    target >= effective && target < effective + width as u64
}

fn host_call_targets(instr: &Instr, store: &Store, exec: &ExecState, inst: u32) -> bool {
    match instr {
        Instr::Call(func) => {
            let Some(instance) = store.instances.get(inst as usize) else {
                return false;
            };
            let Some(addr) = instance.func_addrs.get(*func as usize) else {
                return false;
            };
            matches!(
                store.funcs.get(*addr as usize).map(|f| &f.kind),
                Some(FuncKind::Host(_))
            )
        }
        Instr::CallIndirect { table_idx, .. } => {
            let Some(instance) = store.instances.get(inst as usize) else {
                return false;
            };
            let Some(table_addr) = instance.table_addrs.get(*table_idx as usize) else {
                return false;
            };
            let Some(Value::I32(idx)) = exec.stack.last() else {
                return false;
            };
            let Some(table) = store.tables.get(*table_addr as usize) else {
                return false;
            };
            let Ok(Value::FuncRef(Some(addr))) = table.get(*idx as u32) else {
                return false;
            };
            matches!(
                store.funcs.get(addr as usize).map(|f| &f.kind),
                Some(FuncKind::Host(_))
            )
        }
        _ => false,
    }
}

/// Memory access width helper used by tests and the Studio.
pub fn access_width(instr: &Instr) -> Option<u32> {
    match instr {
        Instr::Load { kind, .. } => Some(match kind {
            LoadKind::I32 | LoadKind::F32 | LoadKind::I64x32S | LoadKind::I64x32U => 4,
            LoadKind::I64 | LoadKind::F64 => 8,
            LoadKind::I32x8S | LoadKind::I32x8U | LoadKind::I64x8S | LoadKind::I64x8U => 1,
            _ => 2,
        }),
        Instr::Store { kind, .. } => Some(match kind {
            StoreKind::I32 | StoreKind::F32 | StoreKind::I64x32 => 4,
            StoreKind::I64 | StoreKind::F64 => 8,
            StoreKind::I32x8 | StoreKind::I64x8 => 1,
            _ => 2,
        }),
        _ => None,
    }
}
