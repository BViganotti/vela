//! JSON view models for the Studio HTTP API.

use vela_core::module::ExportDesc;
use vela_core::runtime::{ExternVal, FuncKind, Value};
use vela_core::spec::{run_wast, WastReport};
use vela_replay::{first_divergence, Breakpoint, Session};

use crate::assets;
use crate::json::Json;
use crate::studio::Studio;

fn hex_hash(h: u64) -> String {
    format!("{h:016x}")
}

pub fn value_json(v: &Value) -> Json {
    let (ty, display) = match v {
        Value::I32(x) => ("i32", x.to_string()),
        Value::I64(x) => ("i64", x.to_string()),
        Value::F32(x) => ("f32", format!("{x}")),
        Value::F64(x) => ("f64", format!("{x}")),
        Value::FuncRef(None) => ("funcref", "null".to_string()),
        Value::FuncRef(Some(a)) => ("funcref", format!("func@{a}")),
        Value::ExternRef(None) => ("externref", "null".to_string()),
        Value::ExternRef(Some(a)) => ("externref", format!("extern@{a}")),
    };
    Json::obj(vec![
        ("type", Json::str(ty)),
        ("display", Json::str(display)),
    ])
}

fn breakpoint_json(bp: &Breakpoint) -> Json {
    match bp {
        Breakpoint::Function { func_addr } => Json::obj(vec![
            ("kind", Json::str("function")),
            ("func", Json::Int(*func_addr as i64)),
        ]),
        Breakpoint::Instruction { func_addr, offset } => Json::obj(vec![
            ("kind", Json::str("instruction")),
            ("func", Json::Int(*func_addr as i64)),
            ("offset", Json::Int(*offset as i64)),
        ]),
        Breakpoint::MemoryAddress(address) => Json::obj(vec![
            ("kind", Json::str("memory")),
            ("address", Json::Int(*address as i64)),
        ]),
        Breakpoint::HostCall => Json::obj(vec![("kind", Json::str("host"))]),
    }
}

fn disasm_for(
    session: &Session,
    func_addr: Option<u32>,
) -> (Option<String>, Option<usize>, Vec<Json>) {
    let Some(addr) = func_addr else {
        return (None, None, Vec::new());
    };
    // Current frame's instruction pointer, if it belongs to this function.
    let current = session
        .exec
        .frames
        .last()
        .filter(|f| f.func_addr == addr)
        .map(|f| f.ip);
    for inst in &session.store.instances {
        let Some(pos) = inst.func_addrs.iter().position(|a| *a == addr) else {
            continue;
        };
        let imported = inst.module.imported_funcs();
        if pos < imported {
            return (Some(format!("import[{pos}]")), current, Vec::new());
        }
        let defined = pos - imported;
        let Some(func) = inst.module.funcs.get(defined) else {
            continue;
        };
        let name = session.func_name(addr);
        let items = func
            .body
            .body
            .iter()
            .enumerate()
            .map(|(i, instr)| {
                Json::obj(vec![
                    ("offset", Json::Int(i as i64)),
                    ("op", Json::str(instr.name())),
                    ("text", Json::str(vela_core::instr::disassemble(instr))),
                    ("current", Json::Bool(current == Some(i))),
                ])
            })
            .collect();
        return (Some(name), current, items);
    }
    (None, current, Vec::new())
}

pub fn state_json(studio: &Studio, memory_start: u32) -> Json {
    let mut pairs: Vec<(&str, Json)> = vec![
        ("source", Json::str(studio.source.clone())),
        ("export", Json::str(studio.export.clone())),
        ("message", Json::str(studio.message.clone())),
        (
            "error",
            match &studio.error {
                Some(e) => Json::str(e.clone()),
                None => Json::Null,
            },
        ),
        (
            "args",
            Json::Arr(
                studio
                    .args
                    .iter()
                    .map(|v| match v {
                        Value::I32(x) => Json::Int(*x as i64),
                        Value::I64(x) => Json::str(format!("i64:{x}")),
                        Value::F32(x) => Json::str(format!("f32:{}", x)),
                        Value::F64(x) => Json::str(format!("f64:{}", x)),
                        Value::FuncRef(None) => Json::str("funcref:null"),
                        Value::ExternRef(None) => Json::str("externref:null"),
                        other => Json::str(other.to_string()),
                    })
                    .collect(),
            ),
        ),
    ];

    let Some(session) = studio.session() else {
        pairs.push(("loaded", Json::Bool(false)));
        return Json::obj(pairs);
    };

    pairs.push(("loaded", Json::Bool(true)));
    pairs.push(("step", Json::Int(session.exec.steps as i64)));
    pairs.push(("finished", Json::Bool(session.exec.finished)));
    pairs.push((
        "trap",
        match &session.exec.trap {
            Some(t) => Json::str(t.to_string()),
            None => Json::Null,
        },
    ));
    pairs.push(("call_depth", Json::Int(session.exec.frames.len() as i64)));
    pairs.push(("stack_height", Json::Int(session.exec.stack.len() as i64)));
    pairs.push(("trace_hash", Json::str(hex_hash(session.trace_hash()))));
    pairs.push(("state_hash", Json::str(hex_hash(session.state_hash()))));
    pairs.push((
        "memory_hash",
        Json::str(hex_hash(session.store.state_hash())),
    ));
    pairs.push(("trace_total", Json::Int(session.trace.len() as i64)));
    pairs.push(("checkpoints", Json::Int(session.checkpoint_count() as i64)));
    pairs.push(("can_back", Json::Bool(session.exec.steps > 0)));

    let func_addr = studio.current_func_addr();
    let (func_name, current_offset, disasm) = disasm_for(session, func_addr);
    pairs.push((
        "current_func",
        match func_addr {
            Some(a) => Json::Int(a as i64),
            None => Json::Null,
        },
    ));
    pairs.push((
        "current_func_name",
        match func_name {
            Some(n) => Json::str(n),
            None => Json::Null,
        },
    ));
    pairs.push((
        "pc",
        match current_offset {
            Some(o) => Json::Int(o as i64),
            None => Json::Null,
        },
    ));
    pairs.push(("disasm", Json::Arr(disasm)));

    pairs.push((
        "stack",
        Json::Arr(session.exec.stack.iter().rev().map(value_json).collect()),
    ));
    pairs.push((
        "frames",
        Json::Arr(
            session
                .exec
                .frames
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    Json::obj(vec![
                        ("depth", Json::Int(i as i64)),
                        ("func", Json::Int(f.func_addr as i64)),
                        ("name", Json::str(session.func_name(f.func_addr))),
                        ("offset", Json::Int(f.ip as i64)),
                        ("locals", Json::Int(f.locals.len() as i64)),
                    ])
                })
                .collect(),
        ),
    ));
    pairs.push((
        "locals",
        Json::Arr(
            session
                .locals()
                .into_iter()
                .map(|(i, v, t)| {
                    Json::obj(vec![
                        ("index", Json::Int(i as i64)),
                        ("type", Json::str(t.name())),
                        ("value", value_json(&v)),
                    ])
                })
                .collect(),
        ),
    ));
    pairs.push((
        "globals",
        Json::Arr(
            session
                .store
                .globals
                .iter()
                .enumerate()
                .map(|(i, g)| {
                    Json::obj(vec![
                        ("index", Json::Int(i as i64)),
                        ("type", Json::str(g.ty().name())),
                        ("mutable", Json::Bool(g.is_mutable())),
                        ("value", value_json(&g.get())),
                    ])
                })
                .collect(),
        ),
    ));

    // Exports of the loaded module instance.
    let mut exports = Vec::new();
    if let Some(inst) = session.store.instances.last() {
        exports = inst
            .exports
            .iter()
            .map(|(name, ext)| {
                let (kind, signature) = match ext {
                    ExternVal::Func(addr) => {
                        let sig = session
                            .store
                            .func(*addr)
                            .map(|f| {
                                format!(
                                    "({}) -> ({})",
                                    f.ty.params
                                        .iter()
                                        .map(|t| t.name())
                                        .collect::<Vec<_>>()
                                        .join(" "),
                                    f.ty.results
                                        .iter()
                                        .map(|t| t.name())
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                )
                            })
                            .unwrap_or_default();
                        ("func", sig)
                    }
                    ExternVal::Global(_) => ("global", String::new()),
                    ExternVal::Memory(_) => ("memory", String::new()),
                    ExternVal::Table(_) => ("table", String::new()),
                };
                Json::obj(vec![
                    ("name", Json::str(name.clone())),
                    ("kind", Json::str(kind)),
                    ("signature", Json::str(signature)),
                ])
            })
            .collect();
    }
    pairs.push(("exports", Json::Arr(exports)));

    // Memory window.
    if let Some(mem) = session.store.memories.first() {
        let len = mem.len() as u32;
        let start = memory_start.min(len.saturating_sub(1));
        let end = (start + 256).min(len);
        let bytes = &mem.data()[start as usize..end as usize];
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        pairs.push((
            "memory",
            Json::obj(vec![
                ("pages", Json::Int(mem.size_pages() as i64)),
                ("bytes", Json::Int(len as i64)),
                ("start", Json::Int(start as i64)),
                ("data", Json::str(hex)),
            ]),
        ));
    } else {
        pairs.push(("memory", Json::Null));
    }

    // Recent trace (chronological order, last 200).
    let n = session.trace.len();
    let start = n.saturating_sub(200);
    let events: Vec<Json> = session.trace.events[start..]
        .iter()
        .map(|e| {
            Json::obj(vec![
                ("step", Json::Int(e.step as i64)),
                ("func", Json::Int(e.func_addr as i64)),
                ("name", Json::str(session.func_name(e.func_addr))),
                ("offset", Json::Int(e.offset as i64)),
                ("op", Json::str(e.op)),
                ("stack", Json::Int(e.stack_height as i64)),
                ("depth", Json::Int(e.call_depth as i64)),
            ])
        })
        .collect();
    pairs.push(("trace", Json::Arr(events)));

    pairs.push((
        "breakpoints",
        Json::Arr(session.breakpoints.iter().map(breakpoint_json).collect()),
    ));
    pairs.push(("memory_first", Json::Int(memory_start as i64)));
    Json::obj(pairs)
}

fn report_json(r: &WastReport) -> Json {
    Json::obj(vec![
        ("file", Json::str(r.file.clone())),
        ("total", Json::Int(r.total as i64)),
        ("passed", Json::Int(r.passed as i64)),
        ("failed", Json::Int(r.failed as i64)),
        (
            "failures",
            Json::Arr(
                r.failures
                    .iter()
                    .map(|f| {
                        Json::obj(vec![
                            ("line", Json::Int(f.line as i64)),
                            ("message", Json::str(f.message.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

pub fn spec_json(custom: Option<&str>) -> Json {
    let reports: Vec<WastReport> = match custom {
        Some(src) => vec![run_wast("custom.wast", src)],
        None => assets::FIXTURES
            .iter()
            .map(|(name, src)| run_wast(name, src))
            .collect(),
    };
    let total: usize = reports.iter().map(|r| r.total).sum();
    let passed: usize = reports.iter().map(|r| r.passed).sum();
    Json::obj(vec![
        ("total", Json::Int(total as i64)),
        ("passed", Json::Int(passed as i64)),
        ("failed", Json::Int((total - passed) as i64)),
        (
            "reports",
            Json::Arr(reports.iter().map(report_json).collect()),
        ),
    ])
}

/// Full trace download with hashes, used by the trace export button.
pub fn trace_json(studio: &Studio) -> Json {
    let Some(session) = studio.session() else {
        return Json::obj(vec![("error", Json::str("no session"))]);
    };
    let events: Vec<Json> = session
        .trace
        .events
        .iter()
        .map(|e| {
            Json::obj(vec![
                ("step", Json::Int(e.step as i64)),
                ("func", Json::Int(e.func_addr as i64)),
                ("offset", Json::Int(e.offset as i64)),
                ("op", Json::str(e.op)),
                ("stack_height", Json::Int(e.stack_height as i64)),
                ("call_depth", Json::Int(e.call_depth as i64)),
                ("stack_hash", Json::str(hex_hash(e.stack_hash))),
            ])
        })
        .collect();
    Json::obj(vec![
        ("export", Json::str(studio.export.clone())),
        ("steps", Json::Int(session.exec.steps as i64)),
        ("trace_hash", Json::str(hex_hash(session.trace_hash()))),
        ("state_hash", Json::str(hex_hash(session.state_hash()))),
        ("events", Json::Arr(events)),
    ])
}

pub fn diff_json(source: &str, export: &str, a: Vec<i32>, b: Vec<i32>) -> Json {
    use vela_core::runtime::Limits;
    let run = |args: Vec<i32>| -> Result<Session, String> {
        Session::from_wat(
            source,
            export,
            args.into_iter().map(Value::I32).collect(),
            Limits {
                max_steps: 300_000,
                max_call_depth: 512,
                max_stack: 200_000,
            },
        )
        .map_err(|e| e.to_string())
    };
    let mut sa = match run(a) {
        Ok(s) => s,
        Err(e) => return Json::obj(vec![("error", Json::str(e))]),
    };
    let mut sb = match run(b) {
        Ok(s) => s,
        Err(e) => return Json::obj(vec![("error", Json::str(e))]),
    };
    let _ = sa.run_to_completion();
    let _ = sb.run_to_completion();
    let divergence = first_divergence(&sa.trace, &sb.trace);
    Json::obj(vec![
        ("error", Json::Null),
        ("diverged", Json::Bool(divergence.is_some())),
        (
            "step",
            divergence
                .as_ref()
                .map(|d| Json::Int(d.step as i64))
                .unwrap_or(Json::Null),
        ),
        (
            "reason",
            divergence
                .as_ref()
                .map(|d| Json::str(d.reason.clone()))
                .unwrap_or(Json::Null),
        ),
        (
            "left_op",
            match divergence.as_ref().and_then(|d| d.left.as_ref()) {
                Some(e) => Json::str(e.op),
                None => Json::Null,
            },
        ),
        (
            "right_op",
            match divergence.as_ref().and_then(|d| d.right.as_ref()) {
                Some(e) => Json::str(e.op),
                None => Json::Null,
            },
        ),
        ("left_steps", Json::Int(sa.exec.steps as i64)),
        ("right_steps", Json::Int(sb.exec.steps as i64)),
        ("left_trace", Json::str(hex_hash(sa.trace_hash()))),
        ("right_trace", Json::str(hex_hash(sb.trace_hash()))),
    ])
}

/// Export names of a module for the UI selector.
pub fn exports_of(source: &str) -> Json {
    match vela_core::text::parse_wat(source) {
        Ok(m) => Json::Arr(
            m.exports
                .iter()
                .filter_map(|e| match e.desc {
                    ExportDesc::Func(_) => Some(Json::str(e.name.clone())),
                    _ => None,
                })
                .collect(),
        ),
        Err(_) => Json::Arr(Vec::new()),
    }
}

/// Describe a function target for help text.
pub fn func_kind(session: &Session, addr: u32) -> &'static str {
    match session.store.func(addr).map(|f| &f.kind) {
        Ok(FuncKind::Host(_)) => "host",
        _ => "wasm",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studio::Studio;

    #[test]
    fn default_state_exposes_the_workbench_model() {
        let mut studio = Studio::new();
        let state = state_json(&studio, 0);
        assert_eq!(state.get("loaded").unwrap().as_bool(), Some(true));
        assert_eq!(state.get("export").unwrap().as_str(), Some("main"));
        assert!(state.get("disasm").unwrap().as_arr().unwrap().len() > 0);
        assert!(state.get("exports").unwrap().as_arr().unwrap().len() >= 3);
        assert!(
            state
                .get("memory")
                .unwrap()
                .get("pages")
                .unwrap()
                .as_i64()
                .unwrap()
                >= 1
        );
        assert_eq!(state.get("step").unwrap().as_i64(), Some(0));
        assert!(state.get("trace_hash").unwrap().as_str().unwrap().len() == 16);

        studio.action("run", None).unwrap();
        let state = state_json(&studio, 0);
        assert_eq!(state.get("finished").unwrap().as_bool(), Some(true));
        assert!(state.get("step").unwrap().as_i64().unwrap() > 100);
        assert!(state.get("trace").unwrap().as_arr().unwrap().len() > 0);

        let trace = trace_json(&studio);
        let events = trace.get("events").unwrap().as_arr().unwrap();
        assert_eq!(
            events.len() as i64,
            trace.get("steps").unwrap().as_i64().unwrap()
        );
    }

    #[test]
    fn memory_window_is_clamped_and_hex_encoded() {
        let studio = Studio::new();
        let state = state_json(&studio, 999_999);
        let mem = state.get("memory").unwrap();
        let start = mem.get("start").unwrap().as_i64().unwrap();
        let data = mem.get("data").unwrap().as_str().unwrap();
        assert!(start >= 0);
        assert!(data.len() <= 512);
        assert!(data.len() % 2 == 0);
        assert!(data.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn spec_dashboard_runs_embedded_fixtures() {
        let report = spec_json(None);
        assert_eq!(report.get("total").unwrap().as_i64(), Some(41));
        assert_eq!(report.get("passed").unwrap().as_i64(), Some(41));
        assert_eq!(report.get("failed").unwrap().as_i64(), Some(0));
        assert_eq!(report.get("reports").unwrap().as_arr().unwrap().len(), 6);
        let custom = spec_json(Some(
            "(module)\n(assert_return (invoke \"x\") (i32.const 0))",
        ));
        assert_eq!(custom.get("failed").unwrap().as_i64(), Some(1));
    }

    #[test]
    fn diff_reports_first_divergence() {
        let src = "(module (func (export \"f\") (param i32) (result i32) local.get 0))";
        let same = diff_json(src, "f", vec![1], vec![1]);
        assert_eq!(same.get("diverged").unwrap().as_bool(), Some(false));
        let diff = diff_json(src, "f", vec![1], vec![2]);
        assert_eq!(diff.get("diverged").unwrap().as_bool(), Some(true));
        assert_eq!(diff.get("step").unwrap().as_i64(), Some(1));
    }

    #[test]
    fn exports_of_lists_only_functions() {
        let src = "(module (memory (export \"m\") 1) (func (export \"f\")) (global (export \"g\") i32 (i32.const 0)))";
        let exports = exports_of(src);
        let names: Vec<&str> = exports
            .as_arr()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["f"]);
        assert!(exports_of("not wat").as_arr().unwrap().is_empty());
    }
}
