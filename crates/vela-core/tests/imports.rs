//! Cross-module linking: shared memories, tables, globals and functions.

use vela_core::runtime::{instantiate, invoke, Registry, Store, Value};
use vela_core::text::parse_wat;

const A: &str = r#"
(module
  (memory (export "mem") 1 4)
  (table (export "tab") 1 funcref)
  (global (export "g") (mut i32) (i32.const 1))
  (type $un (func (result i32)))
  (func $seven (export "seven") (type $un) (result i32) i32.const 7)
  (elem (i32.const 0) $seven)
  (func (export "inc") (result i32)
    global.get 0 i32.const 1 i32.add global.set 0 global.get 0)
  (func (export "write") (param i32 i32)
    local.get 0 local.get 1 i32.store))
"#;

const B: &str = r#"
(module
  (type $un (func (result i32)))
  (import "lib" "mem" (memory 1))
  (import "lib" "tab" (table 1 funcref))
  (import "lib" "g" (global $g (mut i32)))
  (import "lib" "inc" (func $inc (result i32)))
  (import "lib" "seven" (func $seven (type $un) (result i32)))
  (func (export "bump") (result i32)
    call $inc
    call $inc
    i32.add
    global.get $g
    i32.add)
  (func (export "read_mem") (result i32)
    i32.const 0
    i32.load)
  (func (export "call_tab") (result i32)
    i32.const 0
    call_indirect (type $un))
  (func (export "grow") (result i32)
    i32.const 1
    memory.grow)
  (func (export "pages") (result i32)
    memory.size))
"#;

fn call(store: &mut Store, inst: u32, name: &str) -> Vec<Value> {
    let addr = match store.instances[inst as usize].exports[name] {
        vela_core::runtime::ExternVal::Func(a) => a,
        _ => panic!("not a function"),
    };
    invoke(store, addr, vec![], vela_core::runtime::Limits::default()).unwrap()
}

#[test]
fn linked_modules_share_state() {
    let mut store = Store::new();
    let a = instantiate(&mut store, parse_wat(A).unwrap(), &Registry::new()).unwrap();
    let mut registry = Registry::new();
    registry.register_instance("lib", &store.instances[a as usize]);
    let b = instantiate(&mut store, parse_wat(B).unwrap(), &registry).unwrap();

    assert_eq!(call(&mut store, b, "call_tab"), vec![Value::I32(7)]);
    // inc returns 2 then 3; their sum plus the final global value 3 is 8.
    assert_eq!(call(&mut store, b, "bump"), vec![Value::I32(8)]);

    let write = match store.instances[a as usize].exports["write"] {
        vela_core::runtime::ExternVal::Func(x) => x,
        _ => panic!(),
    };
    invoke(
        &mut store,
        write,
        vec![Value::I32(0), Value::I32(0x1234)],
        vela_core::runtime::Limits::default(),
    )
    .unwrap();
    assert_eq!(call(&mut store, b, "read_mem"), vec![Value::I32(0x1234)]);

    assert_eq!(call(&mut store, b, "grow"), vec![Value::I32(1)]);
    assert_eq!(call(&mut store, b, "pages"), vec![Value::I32(2)]);
}

#[test]
fn missing_links_are_rejected() {
    let mut store = Store::new();
    let mut registry = Registry::new();
    instantiate(&mut store, parse_wat(A).unwrap(), &Registry::new()).unwrap();
    // Register only some exports: "mem" is present but "tab" is not.
    let inst = &store.instances[0];
    if let Some(v) = inst.exports.get("mem") {
        registry.insert("lib", "mem", *v);
    }
    let err = instantiate(&mut store, parse_wat(B).unwrap(), &registry).unwrap_err();
    assert!(err.to_string().contains("import"), "{err}");
}
