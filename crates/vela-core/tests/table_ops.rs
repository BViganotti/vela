//! Table instructions: get/set, grow/fill/copy/init, drops and traps.

use vela_core::runtime::{instantiate, invoke, ExternVal, Limits, Registry, Store, Trap, Value};
use vela_core::text::parse_wat;

const WAT: &str = r#"
(module
  (type $un (func (result i32)))
  (type $ii (func (param i32) (result i32)))
  (table 3 funcref)
  (func $a (type $un) (result i32) i32.const 7)
  (func $b (type $un) (result i32) i32.const 9)
  (func $id (type $ii) (param i32) (result i32) local.get 0)
  (elem (i32.const 0) $a $b $id)
  (elem func $a)

  (func (export "pick") (param i32) (result i32)
    local.get 0 call_indirect (type $un))
  (func (export "typeerr") (result i32)
    i32.const 2 call_indirect (type $un))
  (func (export "oob") (result i32)
    i32.const 3 call_indirect (type $un))
  (func (export "getoob") (result i32)
    i32.const 9 table.get 0 ref.is_null)
  (func (export "nullslot") (param i32) (result i32)
    local.get 0 ref.null func table.set 0
    local.get 0 table.get 0 ref.is_null)
  (func (export "size") (result i32) table.size 0)
  (func (export "grow") (result i32)
    ref.null func i32.const 1 table.grow 0 drop
    table.size 0)
  (func (export "growneg") (result i32)
    ref.null func i32.const -1 table.grow 0)
  (func (export "fill_pick") (result i32)
    i32.const 0 ref.func $a i32.const 3 table.fill 0
    i32.const 2 call_indirect (type $un))
  (func (export "fill_neg")
    i32.const 0 ref.null func i32.const -1 table.fill 0)
  (func (export "init_pick") (result i32)
    i32.const 1 i32.const 0 i32.const 1 table.init 0 1
    i32.const 1 call_indirect (type $un))
  (func (export "init_neg")
    i32.const 0 i32.const 0 i32.const -1 table.init 0 1)
  (func (export "copy_neg")
    i32.const 0 i32.const 0 i32.const -1 table.copy 0 0)
  (func (export "drop_then_init")
    elem.drop 1
    i32.const 0 i32.const 0 i32.const 1 table.init 0 1)
)
"#;

fn run(export: &str, args: Vec<Value>) -> Result<Vec<Value>, Trap> {
    let module = parse_wat(WAT).unwrap();
    let mut store = Store::new();
    let inst = instantiate(&mut store, module, &Registry::new()).unwrap();
    let addr = match store.instances[inst as usize].exports[export] {
        ExternVal::Func(a) => a,
        _ => panic!("not a function"),
    };
    invoke(&mut store, addr, args, Limits::default())
}

fn run0(export: &str) -> Result<Vec<Value>, Trap> {
    run(export, vec![])
}

#[test]
fn indirect_calls_and_type_errors() {
    assert_eq!(run("pick", vec![Value::I32(1)]).unwrap(), vec![Value::I32(9)]);
    assert_eq!(run0("typeerr"), Err(Trap::IndirectCallTypeMismatch));
    assert_eq!(run0("oob"), Err(Trap::UndefinedElement));
}

#[test]
fn table_get_set_and_null_slots() {
    assert_eq!(run("nullslot", vec![Value::I32(1)]).unwrap(), vec![Value::I32(1)]);
    assert_eq!(run0("getoob"), Err(Trap::TableOutOfBounds));
    assert_eq!(run0("size").unwrap(), vec![Value::I32(3)]);
}

#[test]
fn table_growth_and_negative_delta() {
    assert_eq!(run0("grow").unwrap(), vec![Value::I32(4)]);
    assert_eq!(run0("growneg").unwrap(), vec![Value::I32(-1)]);
}

#[test]
fn table_fill_init_and_copy() {
    assert_eq!(run0("fill_pick").unwrap(), vec![Value::I32(7)]);
    assert_eq!(run0("init_pick").unwrap(), vec![Value::I32(7)]);
    assert_eq!(run0("fill_neg"), Err(Trap::TableOutOfBounds));
    assert_eq!(run0("init_neg"), Err(Trap::TableOutOfBounds));
    assert_eq!(run0("copy_neg"), Err(Trap::TableOutOfBounds));
}

#[test]
fn dropping_an_element_segment_traps_on_reuse() {
    assert_eq!(run0("drop_then_init"), Err(Trap::ElemSegmentDropped));
}
