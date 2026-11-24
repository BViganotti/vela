//! Replay, reverse stepping, breakpoints and divergence tests.

use vela_core::runtime::{instantiate, invoke_export, Limits, Registry, Store, Value};
use vela_core::text::parse_wat;
use vela_core::validate::validate_module;
use vela_replay::{
    first_divergence, fingerprint as record, Breakpoint, RunStop, Session, SessionError,
};

const LOOP_WAT: &str = r#"
(module
  (memory (export "mem") 1)
  (func (export "count") (param $n i32) (result i32)
    (local $i i32)
    (local $sum i32)
    block $done
      loop $loop
        local.get $i
        local.get $n
        i32.ge_u
        br_if $done
        local.get $sum
        local.get $i
        i32.add
        local.set $sum
        local.get $i
        i32.const 1
        i32.add
        local.tee $i
        i32.const 8
        i32.mul
        i32.const 64
        i32.add
        i32.const 0
        i32.add
        drop
        br $loop
      end
    end
    local.get $sum))
"#;

fn make_session(
    src: &str,
    export: &str,
    args: Vec<Value>,
    interval: u64,
) -> Session {
    let module = parse_wat(src).expect("parse");
    validate_module(&module).expect("validate");
    let mut store = Store::new();
    let inst = instantiate(&mut store, module, &Registry::new()).expect("instantiate");
    Session::from_instance(store, inst, export, args, Limits::default(), interval)
        .expect("session")
}

fn run_to_end(s: &mut Session) -> Vec<Value> {
    match s.run_to_completion() {
        RunStop::Finished => s.exec.results.clone(),
        other => panic!("expected finish, got {other:?}"),
    }
}

#[test]
fn forward_backward_states_match() {
    let mut s = make_session(LOOP_WAT, "count", vec![Value::I32(200)], 1);
    let mut state_hashes = vec![s.state_hash()];
    let mut trace_hashes = vec![s.trace_hash()];
    loop {
        match s.step_forward() {
            vela_core::runtime::StepResult::Running => {
                state_hashes.push(s.state_hash());
                trace_hashes.push(s.trace_hash());
            }
            vela_core::runtime::StepResult::Finished => {
                state_hashes.push(s.state_hash());
                trace_hashes.push(s.trace_hash());
                break;
            }
            vela_core::runtime::StepResult::Trapped(t) => panic!("trap: {t}"),
        }
    }
    assert!(s.exec.steps > 500, "need at least 500 steps, got {}", s.exec.steps);
    let total = s.exec.steps;

    // 500 checkpoints spread over the run, rewound in descending order.
    for k in (0..500u64).rev() {
        let target = total * k / 499;
        s.rewind_to(target).expect("rewind");
        assert_eq!(s.state_hash(), state_hashes[target as usize], "state at {target}");
        assert_eq!(s.trace_hash(), trace_hashes[target as usize], "trace at {target}");
    }
}

#[test]
fn sequential_step_back_matches_forward() {
    let mut s = make_session(LOOP_WAT, "count", vec![Value::I32(4)], 4);
    run_to_end(&mut s);
    let mut hashes = Vec::new();
    let mut temp = make_session(LOOP_WAT, "count", vec![Value::I32(4)], 4);
    hashes.push(temp.state_hash());
    loop {
        match temp.step_forward() {
            vela_core::runtime::StepResult::Running => hashes.push(temp.state_hash()),
            vela_core::runtime::StepResult::Finished => {
                hashes.push(temp.state_hash());
                break;
            }
            vela_core::runtime::StepResult::Trapped(t) => panic!("{t}"),
        }
    }
    for _ in 0..100 {
        let before = s.exec.steps;
        if before == 0 {
            break;
        }
        s.step_back().expect("step back");
        assert_eq!(s.state_hash(), hashes[s.exec.steps as usize]);
    }
}

#[test]
fn hundred_replays_are_identical() {
    let first = record(&mut make_session(LOOP_WAT, "count", vec![Value::I32(30)], 64));
    for i in 0..99 {
        let run = record(&mut make_session(LOOP_WAT, "count", vec![Value::I32(30)], 64));
        assert_eq!(run, first, "replay {i} diverged");
        assert!(run.steps > 0);
        assert_ne!(run.trace_hash, 0);
    }
}

const HOST_WAT: &str = r#"
(module
  (import "env" "tick" (func $tick (result i64)))
  (global $g (mut i64) (i64.const 0))
  (func (export "work") (param $n i32) (result i64)
    block $done
      loop $loop
        local.get $n
        i32.eqz
        br_if $done
        global.get $g
        call $tick
        i64.add
        global.set $g
        local.get $n
        i32.const 1
        i32.sub
        local.set $n
        br $loop
      end
    end
    global.get $g))
"#;

fn host_session() -> Session {
    let module = parse_wat(HOST_WAT).unwrap();
    validate_module(&module).unwrap();
    let mut store = Store::new();
    let mut registry = Registry::new();
    registry.define_host(
        "env",
        "tick",
        vela_core::types::FuncType {
            params: vec![],
            results: vec![vela_core::types::ValType::I64],
        },
        std::rc::Rc::new(|env: &mut vela_core::runtime::HostEnv, _: &[Value]| {
            Ok(vec![Value::I64(env.now_millis() as i64)])
        }),
    );
    registry.install_hosts(&mut store);
    let inst = instantiate(&mut store, module, &registry).unwrap();
    Session::from_instance(store, inst, "work", vec![Value::I32(20)], Limits::default(), 8)
        .unwrap()
}

#[test]
fn host_calls_replay_deterministically() {
    let mut first = host_session();
    let r1 = record(&mut first);
    assert!(r1.steps > 20);
    for _ in 0..100 {
        let mut s = host_session();
        assert_eq!(record(&mut s), r1);
    }
    // Rewinding restores the recorded clock, so replay remains exact.
    let mut s = host_session();
    record(&mut s);
    s.rewind_to(10).unwrap();
    assert_eq!(s.exec.steps, 10);
    let stop = s.run_to_completion();
    let r2 = vela_replay::replay::summarize(&s, stop);
    assert_eq!(r2.trace_hash, r1.trace_hash);
    assert_eq!(r2.state_hash, r1.state_hash);
}

#[test]
fn instruction_breakpoint_stops_before_execution() {
    let mut s = make_session(LOOP_WAT, "count", vec![Value::I32(3)], 4);
    let func_addr = s.exec.frames[0].func_addr;
    s.add_breakpoint(Breakpoint::Instruction { func_addr, offset: 0 });
    let stop = s.run();
    assert!(matches!(stop, RunStop::Breakpoint(Breakpoint::Instruction { offset: 0, .. })));
    assert_eq!(s.exec.steps, 0);
    s.resume();
    assert!(s.exec.steps > 0);
}

#[test]
fn function_breakpoint_triggers_on_entry() {
    let wat = r#"
    (module
      (func $inner (result i32) i32.const 5)
      (func (export "outer") (result i32) call $inner))
    "#;
    let module = parse_wat(wat).unwrap();
    let mut store = Store::new();
    let inst = instantiate(&mut store, module, &Registry::new()).unwrap();
    let inner_addr = store.instances[inst as usize].func_addrs[0];
    let mut s = Session::from_instance(store, inst, "outer", vec![], Limits::default(), 4).unwrap();
    s.add_breakpoint(Breakpoint::Function { func_addr: inner_addr });
    let stop = s.run();
    assert!(matches!(stop, RunStop::Breakpoint(Breakpoint::Function { .. })));
    assert_eq!(s.exec.steps, 1);
    assert_eq!(s.exec.frames.len(), 2);
    assert_eq!(s.exec.frames.last().unwrap().func_addr, inner_addr);
}

#[test]
fn memory_breakpoint_matches_effective_address() {
    let wat = r#"
    (module
      (memory 1)
      (func (export "store") (param i32 i32)
        local.get 0
        local.get 1
        i32.store))
    "#;
    let module = parse_wat(wat).unwrap();
    let mut store = Store::new();
    let inst = instantiate(&mut store, module, &Registry::new()).unwrap();
    let mut s = Session::from_instance(
        store,
        inst,
        "store",
        vec![Value::I32(100), Value::I32(7)],
        Limits::default(),
        4,
    )
    .unwrap();
    s.add_breakpoint(Breakpoint::MemoryAddress(101));
    let stop = s.run();
    assert!(matches!(stop, RunStop::Breakpoint(Breakpoint::MemoryAddress(101))));
    let (_, offset, instr) = s.current_instruction().unwrap();
    assert_eq!(offset, 2);
    assert_eq!(instr.name(), "i32.store");
    s.resume();
    assert!(matches!(s.run(), RunStop::Finished));
}

#[test]
fn host_call_breakpoint() {
    let mut s = host_session();
    s.add_breakpoint(Breakpoint::HostCall);
    let stop = s.run();
    assert!(matches!(stop, RunStop::Breakpoint(Breakpoint::HostCall)));
    assert!(s.exec.steps > 0);
    assert_eq!(s.current_instruction().unwrap().2.name(), "call");
}

#[test]
fn first_divergence_is_reported() {
    let mut a = make_session(LOOP_WAT, "count", vec![Value::I32(4)], 8);
    let mut b = make_session(LOOP_WAT, "count", vec![Value::I32(9)], 8);
    run_to_end(&mut a);
    run_to_end(&mut b);
    let d = first_divergence(&a.trace, &b.trace).expect("traces diverge");
    assert!(d.step < a.exec.steps.min(b.exec.steps));
    assert!(!d.reason.is_empty());
}

#[test]
fn replay_wat_matches_direct_invocation() {
    let src = r#"
    (module
      (func (export "f") (param i32) (result i32)
        (i32.mul (local.get 0) (i32.const 9))))
    "#;
    let module = parse_wat(src).unwrap();
    let mut store = Store::new();
    let inst = instantiate(&mut store, module, &Registry::new()).unwrap();
    let out = invoke_export(&mut store, inst, "f", vec![Value::I32(7)], Limits::default()).unwrap();
    assert_eq!(out, vec![Value::I32(63)]);
    let r = vela_replay::replay_wat(src, "f", vec![Value::I32(7)], Limits::default()).unwrap();
    assert!(r.steps > 0);
    assert_eq!(r.stop, "finished");
}

#[test]
fn cannot_rewind_forward() {
    let mut s = make_session(LOOP_WAT, "count", vec![Value::I32(2)], 4);
    assert!(s.rewind_to(1).is_err());
    s.step_forward();
    assert!(s.rewind_to(2).is_err());
}

#[test]
fn session_reports_errors() {
    let err = Session::from_wat("(module (func (export \"f\") (result i32) i64.const 0))", "f", vec![], Limits::default());
    assert!(matches!(err, Err(SessionError::Instantiate(_))));
    let err = Session::from_wat("(module)", "missing", vec![], Limits::default());
    assert!(matches!(err, Err(SessionError::MissingExport(_))));
}
