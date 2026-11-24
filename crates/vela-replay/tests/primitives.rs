//! Lower-level replay primitives: checkpoints, trace stats, diff and breakpoints.

use vela_core::runtime::{instantiate, Limits, Registry, Store, Value};
use vela_core::text::parse_wat;
use vela_replay::diff::first_divergence;
use vela_replay::record::Trace;
use vela_replay::rewind::CheckpointLog;
use vela_replay::{Breakpoint, RunStop, Session};

const LOOP: &str = r#"
(module
  (memory 1)
  (func (export "f") (param $n i32) (result i32)
    (local $i i32)
    (local $s i32)
    block $done loop $l
      local.get $i local.get $n i32.ge_u br_if $done
      local.get $s local.get $i i32.add local.set $s
      local.get $i i32.const 1 i32.add local.set $i
      br $l
    end end
    local.get $s))
"#;

fn session(interval: u64, n: i32) -> Session {
    let module = parse_wat(LOOP).unwrap();
    let mut store = Store::new();
    let inst = instantiate(&mut store, module, &Registry::new()).unwrap();
    Session::from_instance(store, inst, "f", vec![Value::I32(n)], Limits::default(), interval).unwrap()
}

#[test]
fn checkpoint_log_orders_and_truncates() {
    let mut log = CheckpointLog::new(5);
    assert!(log.nearest(0).is_none());
    let store = Store::new();
    let exec = vela_core::runtime::ExecState::default();
    for step in [0u64, 5, 10, 15] {
        log.maybe_capture(step, &store, &exec, step as usize);
    }
    // maybe_capture with interval 5 records 0,5,10,15.
    assert_eq!(log.len(), 4);
    assert_eq!(log.nearest(12).unwrap().step, 10);
    assert_eq!(log.nearest(99).unwrap().step, 15);
    log.truncate_after(10);
    assert_eq!(log.len(), 3);
    assert_eq!(log.nearest(99).unwrap().step, 10);
    log.clear();
    assert!(log.is_empty());
}

#[test]
fn trace_stats_and_hashes() {
    let mut s = session(4, 20);
    s.run_to_completion();
    let events = s.trace.len();
    assert_eq!(events, s.exec.steps as usize);
    let counts = s.trace.opcode_counts();
    assert!(counts.iter().any(|(op, n)| op == "i32.add" && *n > 0));
    let h1 = s.trace_hash();
    let h2 = {
        let mut s2 = session(4, 20);
        s2.run_to_completion();
        s2.trace_hash()
    };
    assert_eq!(h1, h2);
}

#[test]
fn trace_hash_depends_on_result() {
    let mut a = session(4, 5);
    let mut b = session(4, 6);
    a.run_to_completion();
    b.run_to_completion();
    assert_ne!(a.state_hash(), b.state_hash());
    let d = first_divergence(&a.trace, &b.trace).expect("different workloads diverge");
    assert!(d.reason.len() > 0);
    assert!(d.step < a.exec.steps.min(b.exec.steps));
}

#[test]
fn equal_traces_have_no_divergence() {
    let mut a = session(8, 9);
    let mut b = session(8, 9);
    a.run_to_completion();
    b.run_to_completion();
    assert!(first_divergence(&a.trace, &b.trace).is_none());
    assert!(Trace::new().is_empty());
}

#[test]
fn memory_breakpoint_covers_the_access_width() {
    let wat = r#"
    (module (memory 1)
      (func (export "s") (param i32)
        i32.const 100 i32.const 7 i32.store))
    "#;
    let module = parse_wat(wat).unwrap();
    for (bp, should_hit) in [(99, false), (100, true), (103, true), (104, false)] {
        let mut store = Store::new();
        let inst = instantiate(&mut store, module.clone(), &Registry::new()).unwrap();
        let mut s = Session::from_instance(store, inst, "s", vec![Value::I32(0)], Limits::default(), 4)
            .unwrap();
        s.add_breakpoint(Breakpoint::MemoryAddress(bp));
        let stop = s.run();
        match stop {
            RunStop::Breakpoint(_) => assert!(should_hit, "unexpected hit at {bp}"),
            RunStop::Finished => assert!(!should_hit, "missed breakpoint at {bp}"),
            other => panic!("unexpected stop: {other:?}"),
        }
    }
}

#[test]
fn checkpoint_interval_bounds_snapshots() {
    let mut s = session(5, 40);
    s.run_to_completion();
    let steps = s.exec.steps as usize;
    let cps = s.checkpoint_count();
    assert!(cps > 2, "got {cps} checkpoints");
    assert!(cps < steps, "should not snapshot every step ({cps}/{steps})");
}

#[test]
fn trap_keeps_trace_length_consistent() {
    let wat = r#"
    (module (func (export "boom") (result i32)
      i32.const 1 i32.const 0 i32.div_s))
    "#;
    let mut s = Session::from_wat(wat, "boom", vec![], Limits::default()).unwrap();
    let stop = s.run_to_completion();
    assert!(matches!(stop, RunStop::Trapped(_)));
    assert_eq!(s.trace.len(), s.exec.steps as usize);
    let before = s.state_hash();
    assert!(s.rewind_to(1).is_ok());
    assert_ne!(s.state_hash(), before);
}
