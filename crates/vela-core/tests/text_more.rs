//! Additional text parser/printer coverage: imports, segments, literals.

use vela_core::instr::Instr;
use vela_core::module::{ImportDesc, ElementMode, DataMode};
use vela_core::text::{module_to_wat, parse_wat};
use vela_core::types::{Limits, ValType};
use vela_core::validate::validate_module;

#[test]
fn printer_round_trips_imports_segments_and_exports() {
    let src = r#"
    (module
      (type $t (func (param i32) (result i32)))
      (import "env" "f" (func $f (type $t)))
      (import "env" "t" (table $t0 1 2 funcref))
      (import "env" "m" (memory $m0 1))
      (import "env" "g" (global $g0 i32))
      (table $t1 2 externref)
      (global $g1 (mut i64) (i64.const -1))
      (func $id (type $t) (param $x i32) (result i32) local.get $x)
      (func $start)
      (elem (table 0) (offset (i32.const 0)) func $id)
      (elem func $id)
      (elem declare func $id)
      (elem (table 1) (offset (i32.const 1)) externref (ref.null extern))
      (data (memory 0) (offset (i32.const 4)) "ab\00cd")
      (data "passive")
      (export "id" (func $id))
      (export "memory" (memory 0))
      (start $start))
    "#;
    let m = parse_wat(src).expect("parses");
    validate_module(&m).expect("valid");
    let text = module_to_wat(&m);
    let m2 = parse_wat(&text).expect(&format!("printed text parses:\n{text}"));
    validate_module(&m2).expect("round-trip valid");
    assert_eq!(m.types, m2.types);
    assert_eq!(m.imports.len(), m2.imports.len());
    assert_eq!(m.funcs, m2.funcs);
    assert_eq!(m.tables, m2.tables);
    assert_eq!(m.memories, m2.memories);
    assert_eq!(m.globals, m2.globals);
    assert_eq!(m.exports.len(), m2.exports.len());
    assert_eq!(m.datas, m2.datas);
    assert_eq!(m.elems.len(), m2.elems.len());
    assert_eq!(m.start, m2.start);
}

#[test]
fn integer_and_float_literals() {
    let src = r#"
    (module
      (func (export "a") (result i32) i32.const 0xffffffff)
      (func (export "b") (result i64) i64.const 0xffffffffffffffff)
      (func (export "c") (result i64) i64.const -0x10)
      (func (export "d") (result f32) f32.const 0x1p+4)
      (func (export "e") (result f32) f32.const -nan:0x400001)
      (func (export "f") (result f64) f64.const inf))
    "#;
    let m = parse_wat(src).unwrap();
    validate_module(&m).unwrap();
    let bodies: Vec<Instr> = m.funcs.iter().map(|f| f.body.body[0].clone()).collect();
    assert_eq!(bodies[0], Instr::I32Const(-1));
    assert_eq!(bodies[1], Instr::I64Const(-1));
    assert_eq!(bodies[2], Instr::I64Const(-16));
    assert_eq!(bodies[3], Instr::F32Const(16.0f32.to_bits()));
    match bodies[4] {
        Instr::F32Const(bits) => {
            let v = f32::from_bits(bits);
            assert!(v.is_nan() && v.is_sign_negative());
        }
        _ => panic!("expected f32.const"),
    }
    match bodies[5] {
        Instr::F64Const(bits) => assert_eq!(f64::from_bits(bits), f64::INFINITY),
        _ => panic!("expected f64.const"),
    }
}

#[test]
fn inline_table_element_shorthand() {
    let src = r#"
    (module
      (func $f (result i32) i32.const 9)
      (table funcref (elem $f))
      (type $un (func (result i32)))
      (func (export "call") (result i32)
        i32.const 0 call_indirect (type $un)))
    "#;
    let m = parse_wat(src).unwrap();
    validate_module(&m).unwrap();
    assert_eq!(m.tables.len(), 1);
    assert_eq!(m.tables[0].limits, Limits::new(1, Some(1)));
    assert!(matches!(m.elems[0].mode, ElementMode::Active { table: 0, .. }));
}

#[test]
fn inline_import_and_export_fields() {
    let src = r#"
    (module
      (func $f (import "env" "f") (param i32) (result i32))
      (memory $m (export "mem") 1)
      (global $g (export "g") (mut i32) (i32.const 0))
      (table $t (export "t") 1 funcref))
    "#;
    let m = parse_wat(src).unwrap();
    assert_eq!(m.imports.len(), 1);
    assert!(matches!(m.imports[0].desc, ImportDesc::Func(_)));
    assert_eq!(m.exports.len(), 3);
    validate_module(&m).unwrap();
}

#[test]
fn passive_and_declarative_segment_modes() {
    let src = r#"
    (module
      (func $f)
      (elem declare func $f)
      (elem func $f)
      (data "abc"))
    "#;
    let m = parse_wat(src).unwrap();
    assert!(matches!(m.elems[0].mode, ElementMode::Declarative));
    assert!(matches!(m.elems[1].mode, ElementMode::Passive));
    assert!(matches!(m.datas[0].mode, DataMode::Passive));
    assert_eq!(m.data_count, Some(1));
}

#[test]
fn implicit_multi_value_block_types() {
    let src = r#"
    (module
      (func (export "pair") (result i32 i64)
        block (result i32 i64)
          i32.const 1
          i64.const 2
        end))
    "#;
    let m = parse_wat(src).unwrap();
    validate_module(&m).unwrap();
    match m.funcs[0].body.body[0] {
        Instr::Block(vela_core::types::BlockType::TypeIdx(i)) => {
            assert!(i as usize >= m.types.len() - 1);
        }
        _ => panic!("expected a type-index block"),
    }
}

#[test]
fn rejects_unsupported_or_malformed_fields() {
    assert!(parse_wat("(module (data (memory 0) (offset i32.const 0) \"x\") (func))").is_ok());
    assert!(parse_wat("(module (custom \"x\"))").is_err());
    assert!(parse_wat("(module (func (param i32) (param)))").is_err());
    assert!(parse_wat("(module (func $a) (import \"m\" \"n\" (func)))").is_err());
    assert!(parse_wat("(module (table 1 i32))").is_err());
    assert!(validate_module(&parse_wat("(module (global i32))").unwrap()).is_err());
    assert!(parse_wat("(module (memory 1 2 shared))").is_err());
    assert!(validate_module(&parse_wat("(module (func (result i32) (i32.add (i32.const 1))))").unwrap()).is_err());
}
