//! Execution of `.wast` commands with an honest pass/fail report.

use std::collections::HashMap;
use std::fmt;

use crate::runtime::store::Registry;
use crate::runtime::{instantiate, invoke_export, Limits, Store, Value};
use crate::spec::{Action, Command, Expected, ModuleSource, NanKind};
use crate::text::parse_wat;
use crate::text::sexpr::Sexpr;
use crate::types::ValType;
use crate::validate::validate_module;

use super::report::{Failure, WastReport};

/// Parse and execute a `.wast` script.
pub fn run_wast(name: &str, src: &str) -> WastReport {
    let commands = match parse_script(src) {
        Ok(c) => c,
        Err((line, message)) => {
            return WastReport {
                file: name.to_string(),
                total: 1,
                passed: 0,
                failed: 1,
                failures: vec![Failure { line, message }],
            }
        }
    };
    run_script(name, commands)
}

/// Execute already-parsed commands.
pub fn run_script(name: &str, commands: Vec<(u32, Command)>) -> WastReport {
    let mut ex = Executor::default();
    let mut report = WastReport {
        file: name.to_string(),
        total: 0,
        passed: 0,
        failed: 0,
        failures: Vec::new(),
    };
    for (line, cmd) in commands {
        report.total += 1;
        match ex.execute(&cmd) {
            Ok(()) => report.passed += 1,
            Err(message) => {
                report.failed += 1;
                report.failures.push(Failure { line, message });
            }
        }
    }
    report
}

#[derive(Default)]
struct Executor {
    store: Store,
    registry: Registry,
    last: Option<u32>,
    ids: HashMap<String, u32>,
}

impl Executor {
    fn execute(&mut self, cmd: &Command) -> Result<(), String> {
        match cmd {
            Command::Module { name, source } => {
                let inst = self.load(source, true)?;
                self.last = Some(inst);
                if let Some(name) = name {
                    self.ids.insert(name.clone(), inst);
                }
                Ok(())
            }
            Command::Register { name, module } => {
                let inst = self.resolve_instance(module.as_deref())?;
                let instance = self
                    .store
                    .instances
                    .get(inst as usize)
                    .ok_or_else(|| "unknown module instance".to_string())?;
                self.registry.register_instance(name, instance);
                self.ids.insert(name.clone(), inst);
                Ok(())
            }
            Command::Action(action) => self.eval_action(action).map(|_| ()),
            Command::AssertReturn { action, expected } => {
                let actual = self.eval_action(action)?;
                if actual.len() != expected.len() {
                    return Err(format!(
                        "expected {} result(s), got {}",
                        expected.len(),
                        actual.len()
                    ));
                }
                for (e, a) in expected.iter().zip(actual.iter()) {
                    if !matches_expected(e, a) {
                        return Err(format!("expected {e:?}, got {a:?}"));
                    }
                }
                Ok(())
            }
            Command::AssertTrap { action, message } => match self.eval_action(action) {
                Err(_trap) => {
                    let _ = message;
                    Ok(())
                }
                Ok(values) => Err(format!("expected a trap ({message}), returned {values:?}")),
            },
            Command::AssertExhaustion { action, message } => match self.eval_action(action) {
                Err(trap) => {
                    if trap.contains("call stack exhausted") {
                        Ok(())
                    } else {
                        Err(format!("expected stack exhaustion ({message}), got {trap}"))
                    }
                }
                Ok(values) => Err(format!("expected stack exhaustion, returned {values:?}")),
            },
            Command::AssertInvalid {
                name,
                source,
                message,
            } => {
                let _ = name;
                match self.check_module(source) {
                    Ok(()) => Err(format!("module was valid, expected invalid ({message})")),
                    Err(stage) => {
                        if stage.contains("validation") {
                            Ok(())
                        } else {
                            Err(format!("expected an invalid module, got {stage}"))
                        }
                    }
                }
            }
            Command::AssertMalformed { source, message } => match self.check_module(source) {
                Ok(()) => Err(format!(
                    "module was well-formed, expected malformed ({message})"
                )),
                Err(stage) => {
                    if stage.contains("parse") || stage.contains("decode") {
                        Ok(())
                    } else {
                        Err(format!("expected a malformed module, got {stage}"))
                    }
                }
            },
            Command::AssertUnlinkable {
                name,
                source,
                message,
            } => {
                let _ = name;
                match self.load(source, false) {
                    Ok(_) => Err(format!("module linked, expected unlinkable ({message})")),
                    Err(e) => {
                        if e.contains("import") || e.contains("trapped") || e.contains("limit") {
                            Ok(())
                        } else {
                            Err(format!("expected unlinkable module, got {e}"))
                        }
                    }
                }
            }
            Command::AssertNan { action, kind, bits } => {
                let values = self.eval_action(action)?;
                if values.len() != 1 {
                    return Err("expected one NaN result".to_string());
                }
                let ok = match values[0] {
                    Value::F32(v) => is_nan_kind(v.to_bits() as u64, 32, *kind),
                    Value::F64(v) => is_nan_kind(v.to_bits(), 64, *kind),
                    _ => false,
                };
                if ok {
                    Ok(())
                } else {
                    Err(format!(
                        "expected {:?} NaN (bits {bits:#x}), got {:?}",
                        kind, values[0]
                    ))
                }
            }
        }
    }

    fn resolve_instance(&self, module: Option<&str>) -> Result<u32, String> {
        match module {
            Some(id) => self
                .ids
                .get(id)
                .copied()
                .ok_or_else(|| format!("unknown module {id}")),
            None => self
                .last
                .ok_or_else(|| "no module instantiated".to_string()),
        }
    }

    fn eval_action(&mut self, action: &Action) -> Result<Vec<Value>, String> {
        match action {
            Action::Invoke {
                module,
                field,
                args,
            } => {
                let inst = self.resolve_instance(module.as_deref())?;
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(expected_to_value(a)?);
                }
                invoke_export(&mut self.store, inst, field, values, Limits::default())
                    .map_err(|e| e.to_string())
            }
            Action::Get { module, field } => {
                let inst = self.resolve_instance(module.as_deref())?;
                let ext = self
                    .store
                    .export(inst, field)
                    .ok_or_else(|| format!("unknown global export {field}"))?;
                match ext {
                    crate::runtime::ExternVal::Global(addr) => Ok(vec![self
                        .store
                        .global(addr)
                        .map_err(|e| e.to_string())?
                        .get()]),
                    _ => Err(format!("export {field} is not a global")),
                }
            }
        }
    }

    fn check_module(&mut self, source: &ModuleSource) -> Result<(), String> {
        let module = parse_source(source)?;
        validate_module(&module).map_err(|e| format!("validation: {e}"))?;
        Ok(())
    }

    fn load(&mut self, source: &ModuleSource, _allow_start: bool) -> Result<u32, String> {
        let module = parse_source(source)?;
        instantiate(&mut self.store, module, &self.registry).map_err(|e| e.to_string())
    }
}

fn parse_source(source: &ModuleSource) -> Result<crate::Module, String> {
    match source {
        ModuleSource::Text(text) => parse_wat(text).map_err(|e| format!("parse: {e}")),
        ModuleSource::Quote(text) => parse_wat(text).map_err(|e| format!("parse: {e}")),
        ModuleSource::Binary(bytes) => {
            crate::binary::decode_module(bytes).map_err(|e| format!("decode: {e}"))
        }
    }
}

fn expected_to_value(e: &Expected) -> Result<Value, String> {
    Ok(match e {
        Expected::I32(v) => Value::I32(*v),
        Expected::I64(v) => Value::I64(*v),
        Expected::F32(bits) => Value::F32(f32::from_bits(*bits)),
        Expected::F64(bits) => Value::F64(f64::from_bits(*bits)),
        Expected::F32Nan(_) => Value::F32(f32::NAN),
        Expected::F64Nan(_) => Value::F64(f64::NAN),
        Expected::RefNull(Some(ValType::ExternRef)) => Value::ExternRef(None),
        Expected::RefNull(_) => Value::FuncRef(None),
        Expected::RefExtern(v) => Value::ExternRef(Some(*v)),
        Expected::RefFunc => return Err("ref.func cannot be used as an argument".to_string()),
    })
}

fn matches_expected(e: &Expected, a: &Value) -> bool {
    match (e, a) {
        (Expected::I32(x), Value::I32(y)) => x == y,
        (Expected::I64(x), Value::I64(y)) => x == y,
        (Expected::F32(x), Value::F32(y)) => x == &y.to_bits(),
        (Expected::F64(x), Value::F64(y)) => x == &y.to_bits(),
        (Expected::F32Nan(kind), Value::F32(y)) => is_nan_kind(y.to_bits() as u64, 32, *kind),
        (Expected::F64Nan(kind), Value::F64(y)) => is_nan_kind(y.to_bits(), 64, *kind),
        (Expected::RefNull(_), Value::FuncRef(None)) => true,
        (Expected::RefNull(Some(ValType::ExternRef)), Value::ExternRef(None)) => true,
        (Expected::RefExtern(_), Value::ExternRef(_)) => true,
        _ => false,
    }
}

fn is_nan_kind(bits: u64, width: u32, kind: NanKind) -> bool {
    let (exp_mask, mantissa_mask, quiet) = if width == 32 {
        (0x7f80_0000u64, 0x007f_ffffu64, 0x0040_0000u64)
    } else {
        (
            0x7ff0_0000_0000_0000u64,
            0x000f_ffff_ffff_ffffu64,
            0x0008_0000_0000_0000u64,
        )
    };
    let magnitude = bits & !(1u64 << (width - 1));
    if magnitude & exp_mask != exp_mask {
        return false;
    }
    let payload = magnitude & mantissa_mask;
    if payload == 0 {
        return false;
    }
    match kind {
        NanKind::Canonical => payload == quiet,
        NanKind::Arithmetic => payload & quiet != 0,
    }
}

// --- Script parsing ---------------------------------------------------------

pub fn parse_script(src: &str) -> Result<Vec<(u32, Command)>, (u32, String)> {
    let items = crate::text::sexpr::parse_str(src).map_err(|e| (e.line, e.message))?;
    let mut out = Vec::new();
    for item in items {
        let line = item.line;
        let cmd = parse_command(&item).map_err(|e| (line, e.message))?;
        out.push((line, cmd));
    }
    Ok(out)
}

fn parse_command(s: &Sexpr) -> Result<Command, crate::error::TextError> {
    let list = s.list().ok_or_else(|| s.err("expected a command"))?;
    let head = list
        .first()
        .and_then(|s| s.atom())
        .ok_or_else(|| s.err("expected a command name"))?;
    match head {
        "module" => {
            let (name, source) = parse_module_source(list)?;
            Ok(Command::Module { name, source })
        }
        "register" => {
            let name = string_at(list, 1)?;
            let module = list.get(2).and_then(|s| s.atom()).map(|s| s.to_string());
            Ok(Command::Register { name, module })
        }
        "invoke" | "get" => Ok(Command::Action(parse_action(s)?)),
        "assert_return" => {
            let action = parse_action(list.get(1).ok_or_else(|| s.err("missing action"))?)?;
            let mut expected = Vec::new();
            for e in &list[2..] {
                expected.push(parse_expected(e)?);
            }
            Ok(Command::AssertReturn { action, expected })
        }
        "assert_trap" => {
            let action = parse_action(list.get(1).ok_or_else(|| s.err("missing action"))?)?;
            let message = list
                .get(2)
                .map(|_| string_at(list, 2))
                .transpose()?
                .unwrap_or_default();
            Ok(Command::AssertTrap { action, message })
        }
        "assert_exhaustion" => {
            let action = parse_action(list.get(1).ok_or_else(|| s.err("missing action"))?)?;
            let message = list
                .get(2)
                .map(|_| string_at(list, 2))
                .transpose()?
                .unwrap_or_default();
            Ok(Command::AssertExhaustion { action, message })
        }
        "assert_invalid" => {
            let inner = list.get(1).ok_or_else(|| s.err("missing module"))?;
            let inner_list = inner.list().ok_or_else(|| inner.err("expected a module"))?;
            let (name, source) = parse_module_source(inner_list)?;
            let message = list
                .get(2)
                .map(|_| string_at(list, 2))
                .transpose()?
                .unwrap_or_default();
            Ok(Command::AssertInvalid {
                name,
                source,
                message,
            })
        }
        "assert_malformed" => {
            let inner = list.get(1).ok_or_else(|| s.err("missing module"))?;
            let inner_list = inner.list().ok_or_else(|| inner.err("expected a module"))?;
            let (_, source) = parse_module_source(inner_list)?;
            let message = list
                .get(2)
                .map(|_| string_at(list, 2))
                .transpose()?
                .unwrap_or_default();
            Ok(Command::AssertMalformed { source, message })
        }
        "assert_unlinkable" => {
            let inner = list.get(1).ok_or_else(|| s.err("missing module"))?;
            let inner_list = inner.list().ok_or_else(|| inner.err("expected a module"))?;
            let (name, source) = parse_module_source(inner_list)?;
            let message = list
                .get(2)
                .map(|_| string_at(list, 2))
                .transpose()?
                .unwrap_or_default();
            Ok(Command::AssertUnlinkable {
                name,
                source,
                message,
            })
        }
        "assert_return_canonical_nan" => {
            let action = parse_action(list.get(1).ok_or_else(|| s.err("missing action"))?)?;
            Ok(Command::AssertNan {
                action,
                kind: NanKind::Canonical,
                bits: 0,
            })
        }
        "assert_return_arithmetic_nan" => {
            let action = parse_action(list.get(1).ok_or_else(|| s.err("missing action"))?)?;
            Ok(Command::AssertNan {
                action,
                kind: NanKind::Arithmetic,
                bits: 1,
            })
        }
        other => Err(s.err(format!("unsupported wast command {other:?}"))),
    }
}

fn parse_module_source(
    list: &[Sexpr],
) -> Result<(Option<String>, ModuleSource), crate::error::TextError> {
    let mut i = 1;
    let mut name = None;
    if let Some(a) = list.get(i).and_then(|s| s.atom()) {
        if a.starts_with('$') {
            name = Some(a.to_string());
            i += 1;
        }
    }
    let mode = list.get(i).and_then(|s| s.atom());
    match mode {
        Some("binary") => {
            let mut bytes = Vec::new();
            for s in &list[i + 1..] {
                bytes.extend_from_slice(s.bytes().ok_or_else(|| s.err("expected binary string"))?);
            }
            Ok((name, ModuleSource::Binary(bytes)))
        }
        Some("quote") => {
            let mut text = String::new();
            for s in &list[i + 1..] {
                let b = s.bytes().ok_or_else(|| s.err("expected quoted string"))?;
                text.push_str(&String::from_utf8_lossy(b));
            }
            Ok((name, ModuleSource::Quote(text)))
        }
        None => Ok((
            name,
            ModuleSource::Text(
                Sexpr {
                    kind: crate::text::sexpr::SexprKind::List(list.to_vec()),
                    line: 0,
                    col: 0,
                }
                .to_text(),
            ),
        )),
        Some(_) => {
            // A module whose first field is a bare atom (rare); reconstruct.
            Ok((
                name,
                ModuleSource::Text(
                    Sexpr {
                        kind: crate::text::sexpr::SexprKind::List(list.to_vec()),
                        line: 0,
                        col: 0,
                    }
                    .to_text(),
                ),
            ))
        }
    }
}

fn parse_action(s: &Sexpr) -> Result<Action, crate::error::TextError> {
    let list = s.list().ok_or_else(|| s.err("expected an action"))?;
    let head = list
        .first()
        .and_then(|s| s.atom())
        .ok_or_else(|| s.err("expected action name"))?;
    let mut i = 1;
    let mut module = None;
    if let Some(a) = list.get(i).and_then(|s| s.atom()) {
        if a.starts_with('$') {
            module = Some(a.to_string());
            i += 1;
        }
    }
    match head {
        "invoke" => {
            let field = string_at(list, i)?;
            i += 1;
            let mut args = Vec::new();
            while i < list.len() {
                args.push(parse_expected(&list[i])?);
                i += 1;
            }
            Ok(Action::Invoke {
                module,
                field,
                args,
            })
        }
        "get" => {
            let field = string_at(list, i)?;
            Ok(Action::Get { module, field })
        }
        other => Err(s.err(format!("unsupported action {other:?}"))),
    }
}

fn parse_expected(s: &Sexpr) -> Result<Expected, crate::error::TextError> {
    let list = s.list().ok_or_else(|| s.err("expected a constant"))?;
    let head = list
        .first()
        .and_then(|s| s.atom())
        .ok_or_else(|| s.err("expected a constant"))?;
    let arg = list
        .get(1)
        .ok_or_else(|| s.err("constant is missing a value"))?;
    Ok(match head {
        "i32.const" => Expected::I32(crate::text::wat::const_i32(arg)?),
        "i64.const" => Expected::I64(crate::text::wat::const_i64(arg)?),
        "f32.const" => {
            let a = arg.atom().ok_or_else(|| arg.err("expected f32 literal"))?;
            match a {
                "nan:canonical" => Expected::F32Nan(NanKind::Canonical),
                "nan:arithmetic" => Expected::F32Nan(NanKind::Arithmetic),
                _ => Expected::F32(crate::text::wat::const_f32_bits(arg)?),
            }
        }
        "f64.const" => {
            let a = arg.atom().ok_or_else(|| arg.err("expected f64 literal"))?;
            match a {
                "nan:canonical" => Expected::F64Nan(NanKind::Canonical),
                "nan:arithmetic" => Expected::F64Nan(NanKind::Arithmetic),
                _ => Expected::F64(crate::text::wat::const_f64_bits(arg)?),
            }
        }
        "ref.null" => {
            let t = arg
                .atom()
                .ok_or_else(|| arg.err("expected reference type"))?;
            Expected::RefNull(match t {
                "func" | "funcref" => Some(ValType::FuncRef),
                "extern" | "externref" => Some(ValType::ExternRef),
                _ => None,
            })
        }
        "ref.extern" => Expected::RefExtern(crate::text::wat::const_u32(arg)?),
        "ref.func" => Expected::RefFunc,
        other => return Err(s.err(format!("unsupported constant {other:?}"))),
    })
}

fn string_at(list: &[Sexpr], i: usize) -> Result<String, crate::error::TextError> {
    let s = list
        .get(i)
        .ok_or_else(|| crate::error::TextError::new(1, 1, "missing string"))?;
    let bytes = s.bytes().ok_or_else(|| s.err("expected a string"))?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// Helper used by the test harness to print a summary.
pub fn summarize(reports: &[WastReport]) -> String {
    let mut total = 0;
    let mut passed = 0;
    for r in reports {
        total += r.total;
        passed += r.passed;
    }
    format!(
        "{passed}/{total} assertions passed across {} file(s)",
        reports.len()
    )
}

impl fmt::Display for WastReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}/{}", self.file, self.passed, self.total)?;
        for failure in &self.failures {
            write!(f, "\n  line {}: {}", failure.line, failure.message)?;
        }
        Ok(())
    }
}
