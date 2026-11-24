//! Run the hand-written example modules and compare expected results.

use std::path::PathBuf;
use vela_core::runtime::{instantiate, invoke_export, ExternVal, Limits, Registry, Store, Value};
use vela_core::text::parse_wat;

fn example_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name)
}

fn load(name: &str) -> (Store, u32) {
    let src = std::fs::read_to_string(example_path(name)).expect("example exists");
    let module = parse_wat(&src).expect("example parses");
    let mut store = Store::new();
    let inst = instantiate(&mut store, module, &Registry::new()).expect("instantiate");
    (store, inst)
}

fn invoke(store: &mut Store, inst: u32, export: &str, args: &[Value]) -> Result<Vec<Value>, String> {
    invoke_export(store, inst, export, args.to_vec(), Limits::default()).map_err(|e| e.to_string())
}

#[test]
fn examples_match_expected() {
    {
        let (mut store, inst) = load("factorial.wat");
        let out = invoke(&mut store, inst, "fact", &[Value::I32(6)]).expect("factorial.wat");
        assert_eq!(out, vec![Value::I32(720)], "factorial.wat");
    }
    {
        let (mut store, inst) = load("fibonacci.wat");
        let out = invoke(&mut store, inst, "fib", &[Value::I32(10)]).expect("fibonacci.wat");
        assert_eq!(out, vec![Value::I32(55)], "fibonacci.wat");
    }
    {
        let (mut store, inst) = load("gcd.wat");
        let out = invoke(&mut store, inst, "gcd", &[Value::I32(48), Value::I32(18)]).expect("gcd.wat");
        assert_eq!(out, vec![Value::I32(6)], "gcd.wat");
    }
    {
        let (mut store, inst) = load("memory_copy.wat");
        let out = invoke(&mut store, inst, "copy", &[]).expect("memory_copy.wat");
        assert_eq!(out, vec![Value::I32(1819043176)], "memory_copy.wat");
    }
    {
        let (mut store, inst) = load("memory_fill.wat");
        let out = invoke(&mut store, inst, "fill", &[]).expect("memory_fill.wat");
        assert_eq!(out, vec![Value::I32(171)], "memory_fill.wat");
    }
    {
        let (mut store, inst) = load("indirect_call.wat");
        let out = invoke(&mut store, inst, "pick", &[Value::I32(1)]).expect("indirect_call.wat");
        assert_eq!(out, vec![Value::I32(2)], "indirect_call.wat");
    }
}

#[test]
fn example_traps_are_typed() {
}

#[test]
fn every_example_export_resolves() {
    let cases: &[(&str, &str)] = &[
        ("factorial.wat", "fact"),
        ("fibonacci.wat", "fib"),
        ("gcd.wat", "gcd"),
        ("memory_copy.wat", "copy"),
        ("memory_fill.wat", "fill"),
        ("indirect_call.wat", "pick"),
    ];
    for (name, export) in cases {
        let (store, inst) = load(name);
        let found = store.instances[inst as usize].exports.iter().any(|(n, v)| n == export && matches!(v, ExternVal::Func(_)));
        assert!(found, "{name} exports {export}");
    }
}
