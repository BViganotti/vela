//! Reference values, typed selects, globals and multi-value control flow.

use vela_core::runtime::{instantiate, invoke, ExternVal, Limits, Registry, Store, Trap, Value};
use vela_core::text::parse_wat;

fn run(wat: &str, export: &str, args: Vec<Value>) -> Result<Vec<Value>, Trap> {
    let module = parse_wat(wat).unwrap();
    let mut store = Store::new();
    let inst = instantiate(&mut store, module, &Registry::new()).unwrap();
    let addr = match store.instances[inst as usize].exports[export] {
        ExternVal::Func(a) => a,
        _ => panic!("not a function"),
    };
    invoke(&mut store, addr, args, Limits::default())
}

const REFS: &str = r#"
(module
  (type $un (func (result i32)))
  (table 2 funcref)
  (func $f (export "target") (type $un) (result i32) i32.const 42)
  (global $g (mut funcref) (ref.null func))
  (global $e (mut externref) (ref.null extern))

  (func (export "set_and_call") (result i32)
    ref.func $f
    global.set $g
    i32.const 0
    global.get $g
    table.set 0
    i32.const 0
    call_indirect (type $un))
  (func (export "nulls") (result i32)
    global.get $g ref.is_null
    global.get $e ref.is_null
    i32.add)
  (func (export "sel") (param i32) (result funcref)
    ref.null func
    ref.func $f
    local.get 0
    select (result funcref))
  (func (export "iset") (result i32)
    i32.const 0
    ref.null func
    table.set 0
    i32.const 0
    table.get 0
    ref.is_null))
"#;

#[test]
fn funcref_globals_and_indirect_calls() {
    assert_eq!(run(REFS, "set_and_call", vec![]).unwrap(), vec![Value::I32(42)]);
    assert_eq!(run(REFS, "nulls", vec![]).unwrap(), vec![Value::I32(2)]);
}

#[test]
fn typed_select_on_references() {
    let null = run(REFS, "sel", vec![Value::I32(1)]).unwrap();
    assert_eq!(null, vec![Value::FuncRef(None)]);
    let some = run(REFS, "sel", vec![Value::I32(0)]).unwrap();
    assert!(matches!(some[0], Value::FuncRef(Some(_))));
}

#[test]
fn table_rejects_wrong_reference_type() {
    // Table 0 is funcref; writing an externref value is a type error caught by
    // the validator before execution.
    let wat = r#"
    (module
      (table 1 funcref)
      (func (export "bad") (param externref)
        i32.const 0 local.get 0 table.set 0))
    "#;
    let module = parse_wat(wat).unwrap();
    assert!(vela_core::validate::validate_module(&module).is_err());
}

#[test]
fn iset_writes_null_and_reads_it_back() {
    assert_eq!(run(REFS, "iset", vec![]).unwrap(), vec![Value::I32(1)]);
}

#[test]
fn function_level_branch_returns_results() {
    let wat = r#"
    (module (func (export "f") (result i32)
      i32.const 5
      br 0
      i32.const 9))
    "#;
    assert_eq!(run(wat, "f", vec![]).unwrap(), vec![Value::I32(5)]);
}

#[test]
fn loop_parameters_and_results() {
    let wat = r#"
    (module
      (type $lp (func (param i32) (result i32)))
      (func (export "count") (param i32) (result i32)
        i32.const 0
        loop (type $lp)
          i32.const 1
          i32.add
          local.tee 0
          i32.const 3
          i32.eq
          if
            local.get 0
            return
          end
          local.get 0
          br 0
        end))
    "#;
    assert_eq!(run(wat, "count", vec![Value::I32(0)]).unwrap(), vec![Value::I32(3)]);
}

#[test]
fn multi_value_results_and_blocks() {
    let wat = r#"
    (module
      (func (export "triple") (result i32 i64 f32)
        i32.const 1
        i64.const 2
        f32.const 3
        f32.demote_f64
        ))
    "#;
    // The last instruction expects f64; build a valid variant instead.
    let valid = r#"
    (module
      (func (export "triple") (result i32 i64 f64)
        block (result i32 i64 f64)
          i32.const 1
          i64.const 2
          f64.const 3.5
        end))
    "#;
    let out = run(valid, "triple", vec![]).unwrap();
    assert_eq!(out, vec![Value::I32(1), Value::I64(2), Value::F64(3.5)]);
    assert!(parse_wat(wat).is_ok());
    assert!(vela_core::validate::validate_module(&parse_wat(wat).unwrap()).is_err());
}
