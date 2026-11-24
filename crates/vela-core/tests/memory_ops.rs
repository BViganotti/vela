//! Memory instruction matrix: widths, extension, growth and bounds.

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

fn run0(wat: &str, export: &str) -> Result<Vec<Value>, Trap> {
    run(wat, export, vec![])
}

#[test]
fn narrow_loads_extend_correctly() {
    let wat = r#"
    (module
      (memory 1)
      (func (export "f") (result i32)
        i32.const 0 i32.const 0x80 i32.store8
        i32.const 2 i32.const 0x8000 i32.store16
        i32.const 0 i32.load8_s
        i32.const 2 i32.load16_s
        i32.add
        i32.const 0 i32.load8_u
        i32.add
        i32.const 2 i32.load16_u
        i32.add))
    "#;
    let out = run0(wat, "f").unwrap();
    // -128 + -32768 + 128 + 32768 = 0
    assert_eq!(out, vec![Value::I32(0)]);
}

#[test]
fn wide_loads_and_float_bit_round_trips() {
    let wat = r#"
    (module
      (memory 1)
      (func (export "x") (result i32)
        i32.const 0 i32.const 0x99887766 i32.store
        i32.const 0 i32.load16_u)
      (func (export "y") (result i64)
        i32.const 8 i64.const 0x1122334455667788 i64.store
        i32.const 8 i64.load32_u)
      (func (export "z") (result f64)
        i32.const 16 f64.const 1.5 f64.store
        i32.const 16 f64.load))
    "#;
    assert_eq!(run0(wat, "x").unwrap(), vec![Value::I32(0x7766)]);
    assert_eq!(run0(wat, "y").unwrap(), vec![Value::I64(0x5566_7788)]);
    assert_eq!(run0(wat, "z").unwrap(), vec![Value::F64(1.5)]);
}

#[test]
fn address_plus_offset_cannot_wrap() {
    let wat = r#"
    (module (memory 1)
      (func (export "f") (result i32)
        i32.const 0xfffffff0
        i32.load offset=256))
    "#;
    assert_eq!(run0(wat, "f"), Err(Trap::MemoryOutOfBounds));
}

#[test]
fn negative_growth_is_a_failed_growth() {
    let wat = r#"
    (module (memory 1 2)
      (func (export "grow") (result i32)
        i32.const -1
        memory.grow)
      (func (export "size") (result i32)
        memory.size)
      (func (export "grow5") (result i32)
        i32.const 5
        memory.grow))
    "#;
    assert_eq!(run0(wat, "grow").unwrap(), vec![Value::I32(-1)]);
    assert_eq!(run0(wat, "size").unwrap(), vec![Value::I32(1)]);
    assert_eq!(run0(wat, "grow5").unwrap(), vec![Value::I32(-1)]);
}

#[test]
fn negative_bulk_memory_lengths_trap() {
    let wat = r#"
    (module
      (memory 1)
      (data "abcd")
      (func (export "fill") i32.const 0 i32.const 1 i32.const -1 memory.fill)
      (func (export "copy") i32.const 0 i32.const 0 i32.const -1 memory.copy)
      (func (export "init") i32.const 0 i32.const 0 i32.const -1 memory.init 0))
    "#;
    assert_eq!(run0(wat, "fill"), Err(Trap::MemoryOutOfBounds));
    assert_eq!(run0(wat, "copy"), Err(Trap::MemoryOutOfBounds));
    assert_eq!(run0(wat, "init"), Err(Trap::MemoryOutOfBounds));
}

#[test]
fn bulk_memory_bounds_are_checked_before_writing() {
    let wat = r#"
    (module
      (memory 1)
      (data "abcd")
      (func (export "init_hi") i32.const 65534 i32.const 0 i32.const 4 memory.init 0)
      (func (export "init_lo") i32.const 0 i32.const 3 i32.const 2 memory.init 0)
      (func (export "copy") i32.const 65535 i32.const 0 i32.const 2 memory.copy))
    "#;
    assert_eq!(run0(wat, "init_hi"), Err(Trap::MemoryOutOfBounds));
    assert_eq!(run0(wat, "init_lo"), Err(Trap::MemoryOutOfBounds));
    assert_eq!(run0(wat, "copy"), Err(Trap::MemoryOutOfBounds));
}
