//! Full module validation following the WebAssembly validation algorithm.
//!
//! The implementation uses the operand/control stack algorithm from the
//! specification appendix. Types may be unknown after `unreachable`, which is
//! how the algorithm remains sound while tolerating polymorphic stack states.

use crate::error::ValidationError;
use crate::instr::Instr;
use crate::module::{DataMode, ElementMode, ExportDesc, ImportDesc, Module};
use crate::types::{FuncType, GlobalType, Limits, TableType, ValType, MAX_PAGES};

pub fn validate(module: &Module) -> Result<(), ValidationError> {
    let ctx = ModuleContext::build(module)?;
    ctx.validate_module()
}

struct ModuleContext<'a> {
    module: &'a Module,
    func_types: Vec<u32>,
    table_types: Vec<TableType>,
    memory_types: Vec<crate::types::MemoryType>,
    global_types: Vec<GlobalType>,
    imported_globals: usize,
    refs: Vec<bool>,
    data_count_present: bool,
}

impl<'a> ModuleContext<'a> {
    fn build(module: &'a Module) -> Result<Self, ValidationError> {
        let ntypes = module.types.len();
        for (i, imp) in module.imports.iter().enumerate() {
            if let ImportDesc::Func(t) = imp.desc {
                if t as usize >= ntypes {
                    return Err(err(format!("import {i}: unknown type {t}")));
                }
            }
        }
        for (i, f) in module.funcs.iter().enumerate() {
            if f.type_idx as usize >= ntypes {
                return Err(err(format!("function {i}: unknown type {}", f.type_idx)));
            }
        }

        let func_types = module.func_types();
        let table_types = module.table_types();
        let memory_types = module.memory_types();
        let global_types = module.global_types();
        let imported_globals = module.imported_globals();
        let data_count_present = module.data_count.is_some();

        if memory_types.len() > 1 {
            return Err(err("multiple memories are not supported"));
        }
        for (i, m) in memory_types.iter().enumerate() {
            check_limits(m.limits, MAX_PAGES, "memory", i)?;
        }
        for (i, t) in table_types.iter().enumerate() {
            check_limits(t.limits, crate::types::MAX_TABLE_ENTRIES, "table", i)?;
            if !t.elem.is_ref() {
                return Err(err(format!("table {i} has non-reference element type")));
            }
        }

        let mut refs = vec![false; func_types.len()];
        for exp in &module.exports {
            if let ExportDesc::Func(i) = exp.desc {
                mark_ref(&mut refs, i)?;
            }
        }
        for g in &module.globals {
            for instr in &g.init {
                if let Instr::RefFunc(i) = instr {
                    mark_ref(&mut refs, *i)?;
                }
            }
        }
        for e in &module.elems {
            for item in &e.init {
                for instr in item {
                    if let Instr::RefFunc(i) = instr {
                        mark_ref(&mut refs, *i)?;
                    }
                }
            }
        }

        Ok(ModuleContext {
            module,
            func_types,
            table_types,
            memory_types,
            global_types,
            imported_globals,
            refs,
            data_count_present,
        })
    }

    fn validate_module(&self) -> Result<(), ValidationError> {
        let m = self.module;

        // Globals, element and data segments use constant expressions.
        for (i, g) in m.globals.iter().enumerate() {
            validate_const_expr(
                &g.init,
                g.ty.val,
                self.imported_globals,
                &self.global_types,
                &self.refs,
            )
            .map_err(|e| e.at(None, i))?;
        }

        for (i, e) in m.elems.iter().enumerate() {
            if !e.elem_type.is_ref() {
                return Err(err(format!("element {i}: non-reference element type")));
            }
            for (j, item) in e.init.iter().enumerate() {
                validate_const_expr(
                    item,
                    e.elem_type,
                    self.imported_globals,
                    &self.global_types,
                    &self.refs,
                )
                .map_err(|e| e.at(None, j))?;
            }
            match &e.mode {
                ElementMode::Active { table, offset } => {
                    let tt = self
                        .table_types
                        .get(*table as usize)
                        .ok_or_else(|| err(format!("element {i}: unknown table {table}")))?;
                    if tt.elem != e.elem_type {
                        return Err(err(format!("element {i}: type mismatch with table")));
                    }
                    validate_const_expr(
                        offset,
                        ValType::I32,
                        self.imported_globals,
                        &self.global_types,
                        &self.refs,
                    )?;
                }
                ElementMode::Passive | ElementMode::Declarative => {}
            }
        }

        if let Some(count) = m.data_count {
            if count as usize != m.datas.len() {
                return Err(err("data count does not match data segment count"));
            }
        }
        for (i, d) in m.datas.iter().enumerate() {
            if let DataMode::Active { memory, offset } = &d.mode {
                if *memory as usize >= self.memory_types.len() {
                    return Err(err(format!("data {i}: unknown memory {memory}")));
                }
                validate_const_expr(
                    offset,
                    ValType::I32,
                    self.imported_globals,
                    &self.global_types,
                    &self.refs,
                )?;
            }
        }

        // Exports: unique names and in-range indices.
        for (i, e) in m.exports.iter().enumerate() {
            if m.exports[..i].iter().any(|p| p.name == e.name) {
                return Err(err(format!("duplicate export name {:?}", e.name)));
            }
            let ok = match e.desc {
                ExportDesc::Func(x) => (x as usize) < self.func_types.len(),
                ExportDesc::Table(x) => (x as usize) < self.table_types.len(),
                ExportDesc::Memory(x) => (x as usize) < self.memory_types.len(),
                ExportDesc::Global(x) => (x as usize) < self.global_types.len(),
            };
            if !ok {
                return Err(err(format!("export {:?} has unknown index", e.name)));
            }
        }

        if let Some(start) = m.start {
            let ty = self
                .func_types
                .get(start as usize)
                .ok_or_else(|| err("unknown start function"))?;
            let ft = &m.types[*ty as usize];
            if !ft.params.is_empty() || !ft.results.is_empty() {
                return Err(err("start function must have type [] -> []"));
            }
        }

        for (i, f) in m.funcs.iter().enumerate() {
            let ft = &m.types[f.type_idx as usize];
            validate_body(self, i as u32, ft, &f.body)?;
        }
        Ok(())
    }

    fn func_type_of(&self, func: u32) -> Result<&FuncType, ValidationError> {
        let ty = self
            .func_types
            .get(func as usize)
            .ok_or_else(|| err(format!("unknown function {func}")))?;
        Ok(&self.module.types[*ty as usize])
    }

    fn block_signature(&self, bt: crate::types::BlockType) -> Result<(Vec<ValType>, Vec<ValType>), ValidationError> {
        match bt {
            crate::types::BlockType::Empty => Ok((vec![], vec![])),
            crate::types::BlockType::Value(v) => Ok((vec![], vec![v])),
            crate::types::BlockType::TypeIdx(i) => {
                let ty = self
                    .module
                    .types
                    .get(i as usize)
                    .ok_or_else(|| err(format!("unknown block type {i}")))?;
                Ok((ty.params.clone(), ty.results.clone()))
            }
        }
    }

    fn check_data_index(&self, idx: u32) -> Result<(), ValidationError> {
        if idx as usize >= self.module.datas.len() {
            return Err(err(format!("unknown data segment {idx}")));
        }
        if !self.data_count_present {
            return Err(err("data count section required for memory.init/data.drop"));
        }
        Ok(())
    }

    fn check_elem_index(&self, idx: u32) -> Result<(), ValidationError> {
        if idx as usize >= self.module.elems.len() {
            return Err(err(format!("unknown element segment {idx}")));
        }
        Ok(())
    }

    fn check_table(&self, idx: u32) -> Result<&TableType, ValidationError> {
        self.table_types
            .get(idx as usize)
            .ok_or_else(|| err(format!("unknown table {idx}")))
    }

    fn check_memory(&self, idx: u32) -> Result<(), ValidationError> {
        if (idx as usize) < self.memory_types.len() {
            Ok(())
        } else {
            Err(err(format!("unknown memory {idx}")))
        }
    }
}

fn mark_ref(refs: &mut [bool], i: u32) -> Result<(), ValidationError> {
    match refs.get_mut(i as usize) {
        Some(slot) => {
            *slot = true;
            Ok(())
        }
        None => Err(err(format!("unknown function {i} in reference position"))),
    }
}

fn check_limits(l: Limits, hard: u32, what: &str, i: usize) -> Result<(), ValidationError> {
    if !l.is_valid(hard) {
        return Err(err(format!("{what} {i}: limits out of range")));
    }
    Ok(())
}

fn err(message: impl Into<String>) -> ValidationError {
    ValidationError::new("type mismatch", message)
}

// Validation state is carried in a separate struct so `ModuleContext` stays
// immutable and can be shared with constant-expression checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameKind {
    Func,
    Block,
    Loop,
    If,
    Else,
}

#[derive(Clone, Debug)]
struct CtrlFrame {
    kind: FrameKind,
    start_types: Vec<ValType>,
    end_types: Vec<ValType>,
    height: usize,
    unreachable: bool,
}

struct BodyValidator<'a, 'm> {
    ctx: &'a ModuleContext<'m>,
    vals: Vec<Option<ValType>>,
    ctrl: Vec<CtrlFrame>,
    ip: usize,
}

fn validate_body(
    ctx: &ModuleContext<'_>,
    func: u32,
    ty: &FuncType,
    body: &crate::module::FuncBody,
) -> Result<(), ValidationError> {
    let mut locals = ty.params.clone();
    locals.extend_from_slice(&body.locals);
    let mut v = BodyValidator {
        ctx,
        vals: Vec::new(),
        ctrl: vec![CtrlFrame {
            kind: FrameKind::Func,
            start_types: Vec::new(),
            end_types: ty.results.clone(),
            height: 0,
            unreachable: false,
        }],
        ip: 0,
    };
    for (ip, instr) in body.body.iter().enumerate() {
        if v.ctrl.is_empty() {
            return Err(err("instructions after the function's end")
                .at(Some(func), ip));
        }
        v.ip = ip;
        v.instr(instr, &locals).map_err(|e| e.at(Some(func), ip))?;
    }
    if !v.ctrl.is_empty() {
        return Err(err("function body does not end with end").at(Some(func), body.body.len()));
    }
    if v.vals != ty.results.iter().map(|t| Some(*t)).collect::<Vec<_>>() {
        return Err(err("function result type mismatch").at(Some(func), body.body.len()));
    }
    Ok(())
}

impl<'a, 'm> BodyValidator<'a, 'm> {
    fn frame(&self) -> &CtrlFrame {
        self.ctrl.last().expect("validated control stack is never empty")
    }

    fn frame_mut(&mut self) -> &mut CtrlFrame {
        self.ctrl.last_mut().expect("validated control stack is never empty")
    }

    fn push(&mut self, t: ValType) {
        self.vals.push(Some(t));
    }

    fn push_opts(&mut self, ts: &[ValType]) {
        for t in ts {
            self.vals.push(Some(*t));
        }
    }

    fn pop_any(&mut self) -> Result<Option<ValType>, ValidationError> {
        let height = self.frame().height;
        if self.vals.len() == height {
            if self.frame().unreachable {
                return Ok(None);
            }
            return Err(err("operand stack underflow"));
        }
        Ok(self.vals.pop().unwrap())
    }

    fn pop_expect(&mut self, expected: ValType) -> Result<(), ValidationError> {
        match self.pop_any()? {
            Some(actual) if actual != expected => Err(err(format!(
                "expected {expected} but found {actual}"
            ))),
            _ => Ok(()),
        }
    }

    fn pop_types(&mut self, types: &[ValType]) -> Result<(), ValidationError> {
        for t in types.iter().rev() {
            self.pop_expect(*t)?;
        }
        Ok(())
    }

    fn set_unreachable(&mut self) {
        let height = self.frame().height;
        self.vals.truncate(height);
        self.frame_mut().unreachable = true;
    }

    fn push_ctrl(
        &mut self,
        kind: FrameKind,
        start: Vec<ValType>,
        end: Vec<ValType>,
    ) {
        let height = self.vals.len();
        self.push_opts(&start);
        self.ctrl.push(CtrlFrame { kind, start_types: start, end_types: end, height, unreachable: false });
    }

    fn pop_ctrl(&mut self) -> Result<CtrlFrame, ValidationError> {
        let frame = self.frame().clone();
        self.pop_types(&frame.end_types)?;
        if self.vals.len() != frame.height {
            return Err(err(format!(
                "block leaves {} unexpected value(s) on the stack",
                self.vals.len().saturating_sub(frame.height)
            )));
        }
        self.ctrl.pop();
        Ok(frame)
    }

    fn label_types(&self, depth: u32) -> Result<(Vec<ValType>, bool), ValidationError> {
        let idx = self
            .ctrl
            .len()
            .checked_sub(1 + depth as usize)
            .ok_or_else(|| err(format!("unknown label {depth}")))?;
        let frame = &self.ctrl[idx];
        let tys = if frame.kind == FrameKind::Loop {
            frame.start_types.clone()
        } else {
            frame.end_types.clone()
        };
        Ok((tys, frame.kind == FrameKind::Loop))
    }

    fn instr(&mut self, instr: &Instr, locals: &[ValType]) -> Result<(), ValidationError> {
        use Instr::*;
        match instr {
            Unreachable => self.set_unreachable(),
            Nop => {}
            Block(bt) => {
                let (start, end) = self.ctx.block_signature(*bt)?;
                self.pop_types(&start)?;
                self.push_ctrl(FrameKind::Block, start, end);
            }
            Loop(bt) => {
                let (start, end) = self.ctx.block_signature(*bt)?;
                self.pop_types(&start)?;
                self.push_ctrl(FrameKind::Loop, start, end);
            }
            If(bt) => {
                let (start, end) = self.ctx.block_signature(*bt)?;
                self.pop_expect(ValType::I32)?;
                self.pop_types(&start)?;
                self.push_ctrl(FrameKind::If, start, end);
            }
            Else => {
                if self.frame().kind != FrameKind::If {
                    return Err(err("else without matching if"));
                }
                let frame = self.pop_ctrl()?;
                self.push_ctrl(FrameKind::Else, frame.start_types, frame.end_types);
            }
            End => {
                if self.frame().kind == FrameKind::If && self.frame().start_types != self.frame().end_types {
                    return Err(err("if without else requires matching parameter and result types"));
                }
                let frame = self.pop_ctrl()?;
                self.push_opts(&frame.end_types);
            }
            Br(l) => {
                let (types, _) = self.label_types(*l)?;
                self.pop_types(&types)?;
                self.set_unreachable();
            }
            BrIf(l) => {
                let (types, _) = self.label_types(*l)?;
                self.pop_expect(ValType::I32)?;
                self.pop_types(&types)?;
                self.push_opts(&types);
            }
            BrTable(t) => {
                self.pop_expect(ValType::I32)?;
                let (default_types, _) = self.label_types(t.default)?;
                for target in &t.targets {
                    let (types, _) = self.label_types(*target)?;
                    if types != default_types {
                        return Err(err("br_table targets have inconsistent types"));
                    }
                    self.pop_types(&types)?;
                    // Each target consumes the values; restore them for the
                    // next target check.
                    self.push_opts(&types);
                }
                self.pop_types(&default_types)?;
                self.set_unreachable();
            }
            Return => {
                let results = self.ctrl[0].end_types.clone();
                self.pop_types(&results)?;
                self.set_unreachable();
            }
            Call(f) => {
                let ty = self.ctx.func_type_of(*f)?.clone();
                self.pop_types(&ty.params)?;
                self.push_opts(&ty.results);
            }
            CallIndirect { type_idx, table_idx } => {
                let table = self.ctx.check_table(*table_idx)?;
                if table.elem != ValType::FuncRef {
                    return Err(err("call_indirect requires a funcref table"));
                }
                let ty = self
                    .ctx
                    .module
                    .types
                    .get(*type_idx as usize)
                    .ok_or_else(|| err(format!("unknown type {type_idx}")))?
                    .clone();
                self.pop_expect(ValType::I32)?;
                self.pop_types(&ty.params)?;
                self.push_opts(&ty.results);
            }
            Drop => {
                self.pop_any()?;
            }
            Select => {
                self.pop_expect(ValType::I32)?;
                let t1 = self.pop_any()?;
                let t2 = self.pop_any()?;
                let t = match (t1, t2) {
                    (Some(a), Some(b)) if a != b => {
                        return Err(err("select operands have different types"))
                    }
                    (Some(a), _) => Some(a),
                    (_, Some(b)) => Some(b),
                    (None, None) => None,
                };
                match t {
                    Some(t) => {
                        if !t.is_num() {
                            return Err(err("select requires numeric operands"));
                        }
                        self.push(t);
                    }
                    None => self.vals.push(None),
                }
            }
            SelectT(tys) => {
                if tys.len() != 1 {
                    return Err(err("select with types requires exactly one result type"));
                }
                let t = tys[0];
                self.pop_expect(ValType::I32)?;
                self.pop_expect(t)?;
                self.pop_expect(t)?;
                self.push(t);
            }
            LocalGet(i) => {
                let t = *locals
                    .get(*i as usize)
                    .ok_or_else(|| err(format!("unknown local {i}")))?;
                self.push(t);
            }
            LocalSet(i) => {
                let t = *locals
                    .get(*i as usize)
                    .ok_or_else(|| err(format!("unknown local {i}")))?;
                self.pop_expect(t)?;
            }
            LocalTee(i) => {
                let t = *locals
                    .get(*i as usize)
                    .ok_or_else(|| err(format!("unknown local {i}")))?;
                self.pop_expect(t)?;
                self.push(t);
            }
            GlobalGet(i) => {
                let t = self
                    .ctx
                    .global_types
                    .get(*i as usize)
                    .ok_or_else(|| err(format!("unknown global {i}")))?
                    .val;
                self.push(t);
            }
            GlobalSet(i) => {
                let g = self
                    .ctx
                    .global_types
                    .get(*i as usize)
                    .ok_or_else(|| err(format!("unknown global {i}")))?;
                if !g.mutable {
                    return Err(err(format!("global {i} is immutable")));
                }
                self.pop_expect(g.val)?;
            }
            TableGet(t) => {
                let table = self.ctx.check_table(*t)?;
                self.pop_expect(ValType::I32)?;
                self.push(table.elem);
            }
            TableSet(t) => {
                let table = self.ctx.check_table(*t)?;
                let elem = table.elem;
                self.pop_expect(elem)?;
                self.pop_expect(ValType::I32)?;
            }
            TableInit { elem, table } => {
                let tt = self.ctx.check_table(*table)?;
                let seg = self
                    .ctx
                    .module
                    .elems
                    .get(*elem as usize)
                    .ok_or_else(|| err(format!("unknown element segment {elem}")))?;
                if tt.elem != seg.elem_type {
                    return Err(err("table.init type mismatch"));
                }
                self.pop_expect(ValType::I32)?;
                self.pop_expect(ValType::I32)?;
                self.pop_expect(ValType::I32)?;
            }
            ElemDrop(e) => self.ctx.check_elem_index(*e)?,
            TableCopy { dst, src } => {
                let d = self.ctx.check_table(*dst)?;
                let s = self.ctx.check_table(*src)?;
                if d.elem != s.elem {
                    return Err(err("table.copy type mismatch"));
                }
                self.pop_expect(ValType::I32)?;
                self.pop_expect(ValType::I32)?;
                self.pop_expect(ValType::I32)?;
            }
            TableGrow(t) => {
                let table = self.ctx.check_table(*t)?;
                let elem = table.elem;
                self.pop_expect(ValType::I32)?;
                self.pop_expect(elem)?;
                self.push(ValType::I32);
            }
            TableSize(t) => {
                self.ctx.check_table(*t)?;
                self.push(ValType::I32);
            }
            TableFill(t) => {
                let table = self.ctx.check_table(*t)?;
                let elem = table.elem;
                self.pop_expect(ValType::I32)?;
                self.pop_expect(elem)?;
                self.pop_expect(ValType::I32)?;
            }
            MemorySize(m) => {
                self.ctx.check_memory(*m)?;
                self.push(ValType::I32);
            }
            MemoryGrow(m) => {
                self.ctx.check_memory(*m)?;
                self.pop_expect(ValType::I32)?;
                self.push(ValType::I32);
            }
            MemoryInit { data, mem } => {
                self.ctx.check_memory(*mem)?;
                self.ctx.check_data_index(*data)?;
                self.pop_expect(ValType::I32)?;
                self.pop_expect(ValType::I32)?;
                self.pop_expect(ValType::I32)?;
            }
            DataDrop(d) => {
                self.ctx.check_data_index(*d)?;
            }
            MemoryCopy { dst, src } => {
                self.ctx.check_memory(*dst)?;
                self.ctx.check_memory(*src)?;
                self.pop_expect(ValType::I32)?;
                self.pop_expect(ValType::I32)?;
                self.pop_expect(ValType::I32)?;
            }
            MemoryFill(m) => {
                self.ctx.check_memory(*m)?;
                self.pop_expect(ValType::I32)?;
                self.pop_expect(ValType::I32)?;
                self.pop_expect(ValType::I32)?;
            }
            Load { kind, mem, arg } => {
                self.ctx.check_memory(*mem)?;
                check_align(arg.align, kind.width(), kind.name())?;
                self.pop_expect(ValType::I32)?;
                self.push(kind.result_type());
            }
            Store { kind, mem, arg } => {
                self.ctx.check_memory(*mem)?;
                check_align(arg.align, kind.width(), kind.name())?;
                self.pop_expect(kind.value_type())?;
                self.pop_expect(ValType::I32)?;
            }
            I32Const(_) => self.push(ValType::I32),
            I64Const(_) => self.push(ValType::I64),
            F32Const(_) => self.push(ValType::F32),
            F64Const(_) => self.push(ValType::F64),
            RefNull(t) => {
                if !t.is_ref() {
                    return Err(err("ref.null requires a reference type"));
                }
                self.push(*t);
            }
            RefIsNull => {
                match self.pop_any()? {
                    Some(t) if !t.is_ref() => {
                        return Err(err("ref.is_null requires a reference operand"))
                    }
                    _ => {}
                }
                self.push(ValType::I32);
            }
            RefFunc(f) => {
                if *f as usize >= self.ctx.func_types.len() {
                    return Err(err(format!("unknown function {f}")));
                }
                if !self.ctx.refs[*f as usize] {
                    return Err(err(format!("function {f} is not declared in refs")));
                }
                self.push(ValType::FuncRef);
            }
            other => {
                let (params, results) = other
                    .numeric_signature()
                    .ok_or_else(|| err(format!("unhandled instruction {}", other.name())))?;
                self.pop_types(params)?;
                self.push_opts(results);
            }
        }
        Ok(())
    }
}

fn check_align(align: u32, width: u32, name: &str) -> Result<(), ValidationError> {
    if align > 31 {
        return Err(err(format!("{name}: alignment too large")));
    }
    let max_align = width.trailing_zeros();
    if align > max_align {
        return Err(err(format!(
            "{name}: alignment {align} exceeds natural alignment {max_align}"
        )));
    }
    Ok(())
}

/// Validate a constant expression producing exactly `expected`.
pub fn validate_const_expr(
    expr: &[Instr],
    expected: ValType,
    imported_globals: usize,
    global_types: &[GlobalType],
    refs: &[bool],
) -> Result<(), ValidationError> {
    let mut stack: Vec<ValType> = Vec::new();
    let mut ended = false;
    for instr in expr {
        if ended {
            return Err(err("constant expression has trailing instructions"));
        }
        match instr {
            Instr::End => ended = true,
            Instr::I32Const(_) => stack.push(ValType::I32),
            Instr::I64Const(_) => stack.push(ValType::I64),
            Instr::F32Const(_) => stack.push(ValType::F32),
            Instr::F64Const(_) => stack.push(ValType::F64),
            Instr::RefNull(t) => {
                if !t.is_ref() {
                    return Err(err("ref.null requires a reference type"));
                }
                stack.push(*t);
            }
            Instr::RefFunc(f) => {
                if *f as usize >= refs.len() {
                    return Err(err(format!("unknown function {f}")));
                }
                if !refs[*f as usize] {
                    return Err(err(format!("function {f} is not declared in refs")));
                }
                stack.push(ValType::FuncRef);
            }
            Instr::GlobalGet(i) => {
                if *i as usize >= imported_globals {
                    return Err(err("constant expression may only read imported globals"));
                }
                let g = global_types
                    .get(*i as usize)
                    .ok_or_else(|| err("unknown global in constant expression"))?;
                if g.mutable {
                    return Err(err("constant expression reads a mutable global"));
                }
                stack.push(g.val);
            }
            other => {
                return Err(err(format!(
                    "{} is not allowed in a constant expression",
                    other.name()
                )))
            }
        }
    }
    if !ended {
        return Err(err("constant expression is missing end"));
    }
    if stack.len() != 1 || stack[0] != expected {
        return Err(err(format!(
            "constant expression must produce exactly one {expected}"
        )));
    }
    Ok(())
}

/// Re-exported for the runtime instantiation checks.
pub fn validate_limits(limits: Limits, hard_max: u32) -> bool {
    limits.is_valid(hard_max)
}

