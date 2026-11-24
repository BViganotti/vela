//! Text parser tests: modules, folded expressions, segments and errors.

use vela_core::binary::{decode_module, encode_module};
use vela_core::runtime::{
    instantiate, invoke, Limits, Registry, Store, Trap, Value,
};
use vela_core::text::parse_wat;
use vela_core::validate::validate_module;

fn parse_valid(src: &str) -> vela_core::Module {
    let m = parse_wat(src).expect("parse");
    validate_module(&m).expect("validate");
    m
}

fn run(src: &str, name: &str, args: Vec<Value>) -> Result<Vec<Value>, Trap> {
    let m = parse_valid(src);
    let mut store = Store::new();
    let inst = instantiate(&mut store, m, &Registry::new()).expect("instantiate");
    let addr = store
        .instances[inst as usize]
        .exports
        .get(name)
        .and_then(|e| match e {
            vela_core::runtime::ExternVal::Func(a) => Some(*a),
            _ => None,
        })
        .expect("export");
    invoke(&mut store, addr, args, Limits::default())
}

#[test]
fn module_with_add() {
    let src = r#"
        (module
          (type $bin (func (param i32 i32) (result i32)))
          (func $add (export "add") (type $bin) (param $a i32) (param $b i32)
            local.get $a
            local.get $b
            i32.add))
    "#;
    assert_eq!(
        run(src, "add", vec![Value::I32(20), Value::I32(22)]).unwrap(),
        vec![Value::I32(42)]
    );
}

#[test]
fn folded_expressions() {
    let src = r#"
        (module
          (func (export "f") (param $x i32) (result i32)
            (i32.add (i32.mul (local.get $x) (i32.const 2)) (i32.const 1))))
    "#;
    assert_eq!(run(src, "f", vec![Value::I32(20)]).unwrap(), vec![Value::I32(41)]);
}

#[test]
fn labels_and_loops() {
    let src = r#"
        (module
          (func (export "fact") (param $n i32) (result i32)
            (local $acc i32)
            i32.const 1
            local.set $acc
            block $done
              loop $again
                local.get $n
                i32.eqz
                br_if $done
                local.get $acc
                local.get $n
                i32.mul
                local.set $acc
                local.get $n
                i32.const 1
                i32.sub
                local.set $n
                br $again
              end
            end
            local.get $acc))
    "#;
    assert_eq!(run(src, "fact", vec![Value::I32(6)]).unwrap(), vec![Value::I32(720)]);
}

#[test]
fn folded_if_then_else() {
    let src = r#"
        (module
          (func (export "abs") (param $x i32) (result i32)
            (if (result i32)
              (i32.lt_s (local.get $x) (i32.const 0))
              (then (i32.sub (i32.const 0) (local.get $x)))
              (else (local.get $x)))))
    "#;
    assert_eq!(run(src, "abs", vec![Value::I32(-9)]).unwrap(), vec![Value::I32(9)]);
    assert_eq!(run(src, "abs", vec![Value::I32(9)]).unwrap(), vec![Value::I32(9)]);
}

#[test]
fn memory_table_globals_and_data() {
    let src = r#"
        (module
          (memory $mem (export "mem") 1 2)
          (table $tab 2 funcref)
          (global $g (mut i32) (i32.const 7))
          (data (i32.const 16) "\01\02\03\04")
          (type $un (func (result i32)))
          (func $one (result i32) i32.const 1)
          (func $two (result i32) i32.const 2)
          (elem (i32.const 0) $one $two)
          (func (export "load") (result i32)
            i32.const 16
            i32.load)
          (func (export "grow") (result i32)
            global.get $g
            i32.const 1
            i32.add
            global.set $g
            global.get $g)
          (func (export "pick") (param $i i32) (result i32)
            local.get $i
            call_indirect (type $un)))
    "#;
    assert_eq!(run(src, "load", vec![]).unwrap(), vec![Value::I32(0x04030201)]);
    assert_eq!(run(src, "grow", vec![]).unwrap(), vec![Value::I32(8)]);
    assert_eq!(run(src, "pick", vec![Value::I32(1)]).unwrap(), vec![Value::I32(2)]);
}

#[test]
fn imports_and_exports_by_name() {
    let src = r#"
        (module
          (import "env" "twice" (func $twice (param i32) (result i32)))
          (func (export "run") (param i32) (result i32)
            (call $twice (call $twice (local.get 0)))))
    "#;
    let m = parse_valid(src);
    let mut store = Store::new();
    let mut reg = Registry::new();
    reg.define_host(
        "env",
        "twice",
        vela_core::types::FuncType {
            params: vec![vela_core::types::ValType::I32],
            results: vec![vela_core::types::ValType::I32],
        },
        std::rc::Rc::new(|_: &mut vela_core::runtime::HostEnv, args: &[Value]| {
            match args[0] {
                Value::I32(v) => Ok(vec![Value::I32(v * 2)]),
                _ => Err(Trap::TypeMismatch("host arg")),
            }
        }),
    );
    reg.install_hosts(&mut store);
    let inst = instantiate(&mut store, m, &reg).unwrap();
    let addr = match store.instances[inst as usize].exports["run"] {
        vela_core::runtime::ExternVal::Func(a) => a,
        _ => panic!(),
    };
    assert_eq!(
        invoke(&mut store, addr, vec![Value::I32(5)], Limits::default()).unwrap(),
        vec![Value::I32(20)]
    );
}

#[test]
fn binary_round_trip_of_text_module() {
    let src = r#"
        (module
          (memory 1)
          (global $g i32 (i32.const 3))
          (func $f (export "f") (param $x i32) (result i32)
            local.get $x
            global.get $g
            i32.mul))
    "#;
    let m = parse_valid(src);
    let bytes = encode_module(&m);
    let decoded = decode_module(&bytes).expect("decode");
    validate_module(&decoded).expect("validate decoded");
    assert_eq!(m.types, decoded.types);
    assert_eq!(m.funcs, decoded.funcs);
    assert_eq!(m.globals, decoded.globals);
}

#[test]
fn hex_and_float_literals() {
    let src = r#"
        (module
          (func (export "i") (result i32) i32.const 0x2a)
          (func (export "f") (result f64) f64.const 0x1.8p1)
          (func (export "nan") (result f32) f32.const nan:0x400000))
    "#;
    assert_eq!(run(src, "i", vec![]).unwrap(), vec![Value::I32(42)]);
    assert_eq!(run(src, "f", vec![]).unwrap(), vec![Value::F64(3.0)]);
    match run(src, "nan", vec![]).unwrap()[0] {
        Value::F32(v) => assert!(v.is_nan()),
        _ => panic!(),
    }
}

#[test]
fn passive_data_and_memory_init() {
    let src = r#"
        (module
          (memory 1)
          (data "hello")
          (func (export "init") (result i32)
            i32.const 0
            i32.const 0
            i32.const 5
            memory.init 0
            i32.const 0
            i32.load8_u))
    "#;
    assert_eq!(run(src, "init", vec![]).unwrap(), vec![Value::I32(b'h' as i32)]);
}

#[test]
fn rejects_bad_text() {
    assert!(parse_wat("(module (func i32.nope))").is_err());
    assert!(parse_wat("(module (func local.get $missing))").is_err());
    assert!(parse_wat("(module (func (local.get 0)))").is_err());
    assert!(parse_wat("(module (func block end end))").is_err());
    assert!(parse_wat("(module (memory 2 1))").is_err());
    assert!(parse_wat("(module (import \"a\" \"b\" (func 0)))").is_err());
    assert!(validate_module(&parse_wat("(module (func (result i32) i32.const 1 i32.const 2))").unwrap()).is_err());
}

#[test]
fn rejects_invalid_module_after_parse() {
    // Parses, but fails validation: i64.add with i32 operands.
    let m = parse_wat("(module (func (result i64) i32.const 1 i64.add))").unwrap();
    assert!(validate_module(&m).is_err());
}

#[test]
fn start_function_from_text() {
    let src = r#"
        (module
          (memory 1)
          (func $init i32.const 4 i32.const 99 i32.store)
          (start $init)
          (func (export "read") (result i32) i32.const 4 i32.load))
    "#;
    assert_eq!(run(src, "read", vec![]).unwrap(), vec![Value::I32(99)]);
}

#[test]
fn multi_value_block_type() {
    let src = r#"
        (module
          (type $pair (func (result i32 i64)))
          (func (export "pair") (result i32 i64)
            block (type $pair)
              i32.const 1
              i64.const 2
            end))
    "#;
    assert_eq!(
        run(src, "pair", vec![]).unwrap(),
        vec![Value::I32(1), Value::I64(2)]
    );
}
