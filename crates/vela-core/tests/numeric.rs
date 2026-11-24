//! Numeric semantics: edge cases, traps, NaN and rounding behavior.

use vela_core::instr::{Instr, LoadKind, MemArg};
use vela_core::module::{Export, ExportDesc, Func, FuncBody, Module};
use vela_core::runtime::{instantiate, invoke, Limits, Registry, Store, Trap, Value};
use vela_core::types::{FuncType, MemoryType, ValType, Limits as WLimits};
use Instr::*;

/// Evaluate a function and return its results.
fn exec(params: Vec<ValType>, results: Vec<ValType>, body: Vec<Instr>, args: Vec<Value>) -> Result<Vec<Value>, Trap> {
    let mut m = Module::default();
    m.types.push(FuncType { params, results });
    let mut body = body;
    body.push(End);
    m.funcs.push(Func { type_idx: 0, body: FuncBody { locals: vec![], body } });
    m.exports.push(Export { name: "f".into(), desc: ExportDesc::Func(0) });
    let mut store = Store::new();
    let inst = instantiate(&mut store, m, &Registry::new()).unwrap();
    let addr = match store.export(inst, "f").unwrap() {
        vela_core::runtime::ExternVal::Func(a) => a,
        _ => unreachable!(),
    };
    invoke(&mut store, addr, args, Limits::default())
}

fn i32_bin(body: Vec<Instr>, a: i32, b: i32) -> Result<Vec<Value>, Trap> {
    exec(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
        body,
        vec![Value::I32(a), Value::I32(b)],
    )
}

fn i64_bin(body: Vec<Instr>, a: i64, b: i64) -> Result<Vec<Value>, Trap> {
    exec(
        vec![ValType::I64, ValType::I64],
        vec![ValType::I64],
        body,
        vec![Value::I64(a), Value::I64(b)],
    )
}

fn f64_bin(body: Vec<Instr>, a: f64, b: f64) -> Result<Vec<Value>, Trap> {
    exec(
        vec![ValType::F64, ValType::F64],
        vec![ValType::F64],
        body,
        vec![Value::F64(a), Value::F64(b)],
    )
}

fn f32_un(body: Vec<Instr>, a: f32) -> Result<Vec<Value>, Trap> {
    exec(vec![ValType::F32], vec![ValType::F32], body, vec![Value::F32(a)])
}

fn f64_un(body: Vec<Instr>, a: f64) -> Result<Vec<Value>, Trap> {
    exec(vec![ValType::F64], vec![ValType::F64], body, vec![Value::F64(a)])
}

#[test]
fn bit_counts() {
    assert_eq!(
        exec(vec![ValType::I32], vec![ValType::I32], vec![LocalGet(0), I32Clz], vec![Value::I32(1)]).unwrap(),
        vec![Value::I32(31)]
    );
    assert_eq!(
        exec(vec![ValType::I32], vec![ValType::I32], vec![LocalGet(0), I32Ctz], vec![Value::I32(8)]).unwrap(),
        vec![Value::I32(3)]
    );
    assert_eq!(
        exec(vec![ValType::I32], vec![ValType::I32], vec![LocalGet(0), I32Popcnt], vec![Value::I32(-1)]).unwrap(),
        vec![Value::I32(32)]
    );
    assert_eq!(
        exec(vec![ValType::I64], vec![ValType::I64], vec![LocalGet(0), I64Clz], vec![Value::I64(1)]).unwrap(),
        vec![Value::I64(63)]
    );
}

#[test]
fn wrapping_arithmetic() {
    assert_eq!(i32_bin(vec![LocalGet(0), LocalGet(1), I32Add], i32::MAX, 1).unwrap(), vec![Value::I32(i32::MIN)]);
    assert_eq!(i32_bin(vec![LocalGet(0), LocalGet(1), I32Mul], 0x10000, 0x10000).unwrap(), vec![Value::I32(0)]);
    assert_eq!(i64_bin(vec![LocalGet(0), LocalGet(1), I64Sub], i64::MIN, 1).unwrap(), vec![Value::I64(i64::MAX)]);
}

#[test]
fn division_and_remainder() {
    assert_eq!(i32_bin(vec![LocalGet(0), LocalGet(1), I32RemS], i32::MIN, -1).unwrap(), vec![Value::I32(0)]);
    assert_eq!(i32_bin(vec![LocalGet(0), LocalGet(1), I32RemU], -1, 2).unwrap(), vec![Value::I32(1)]);
    assert_eq!(i32_bin(vec![LocalGet(0), LocalGet(1), I32DivU], -1, 2).unwrap(), vec![Value::I32(0x7fff_ffff)]);
    assert_eq!(i64_bin(vec![LocalGet(0), LocalGet(1), I64DivS], -9, 2).unwrap(), vec![Value::I64(-4)]);
    assert_eq!(i64_bin(vec![LocalGet(0), LocalGet(1), I64RemS], -9, 2).unwrap(), vec![Value::I64(-1)]);
}

#[test]
fn shift_masks_counts() {
    assert_eq!(i32_bin(vec![LocalGet(0), LocalGet(1), I32Shl], 1, 33).unwrap(), vec![Value::I32(2)]);
    assert_eq!(i32_bin(vec![LocalGet(0), LocalGet(1), I32ShrU], -1, 28).unwrap(), vec![Value::I32(15)]);
    assert_eq!(i32_bin(vec![LocalGet(0), LocalGet(1), I32Rotl], 0x8000_0001u32 as i32, 1).unwrap(), vec![Value::I32(3)]);
    assert_eq!(i64_bin(vec![LocalGet(0), LocalGet(1), I64Shl], 1, 65).unwrap(), vec![Value::I64(2)]);
}

#[test]
fn float_min_max_signed_zero_and_nan() {
    let min = f64_bin(vec![LocalGet(0), LocalGet(1), F64Min], -0.0, 0.0).unwrap();
    assert_eq!(min[0], Value::F64(-0.0));
    let max = f64_bin(vec![LocalGet(0), LocalGet(1), F64Max], -0.0, 0.0).unwrap();
    assert_eq!(max[0], Value::F64(0.0));
    let nan = f64_bin(vec![LocalGet(0), LocalGet(1), F64Min], f64::NAN, 1.0).unwrap();
    match nan[0] {
        Value::F64(v) => assert!(v.is_nan()),
        _ => panic!(),
    }
    let f32_nan = exec(
        vec![ValType::F32, ValType::F32],
        vec![ValType::F32],
        vec![LocalGet(0), LocalGet(1), F32Max],
        vec![Value::F32(f32::NAN), Value::F32(2.0)],
    )
    .unwrap();
    match f32_nan[0] {
        Value::F32(v) => assert!(v.is_nan()),
        _ => panic!(),
    }
}

#[test]
fn float_rounding() {
    assert_eq!(f64_un(vec![LocalGet(0), F64Nearest], 2.5).unwrap(), vec![Value::F64(2.0)]);
    assert_eq!(f64_un(vec![LocalGet(0), F64Nearest], 3.5).unwrap(), vec![Value::F64(4.0)]);
    assert_eq!(f64_un(vec![LocalGet(0), F64Nearest], -0.5).unwrap(), vec![Value::F64(-0.0)]);
    assert_eq!(f64_un(vec![LocalGet(0), F64Ceil], -0.5).unwrap(), vec![Value::F64(-0.0)]);
    assert_eq!(f32_un(vec![LocalGet(0), F32Floor], 1.9).unwrap(), vec![Value::F32(1.0)]);
    assert_eq!(f64_un(vec![LocalGet(0), F64Sqrt], 81.0).unwrap(), vec![Value::F64(9.0)]);
}

#[test]
fn copysign_and_abs() {
    assert_eq!(f64_bin(vec![LocalGet(0), LocalGet(1), F64Copysign], 3.0, -0.0).unwrap(), vec![Value::F64(-3.0)]);
    assert_eq!(f64_un(vec![LocalGet(0), F64Abs], -7.5).unwrap(), vec![Value::F64(7.5)]);
    assert_eq!(f32_un(vec![LocalGet(0), F32Neg], 2.0).unwrap(), vec![Value::F32(-2.0)]);
}

#[test]
fn truncation_traps() {
    let t = |x: f64| {
        exec(
            vec![ValType::F64],
            vec![ValType::I32],
            vec![LocalGet(0), I32TruncF64S],
            vec![Value::F64(x)],
        )
    };
    assert_eq!(t(f64::NAN), Err(Trap::InvalidConversionToInteger));
    assert_eq!(t(2147483648.0), Err(Trap::IntegerOverflow));
    assert_eq!(t(-2147483649.0), Err(Trap::IntegerOverflow));
    assert_eq!(t(2147483647.9).unwrap(), vec![Value::I32(2147483647)]);
    assert_eq!(t(-3.9).unwrap(), vec![Value::I32(-3)]);

    let u = |x: f64| {
        exec(
            vec![ValType::F64],
            vec![ValType::I32],
            vec![LocalGet(0), I32TruncF64U],
            vec![Value::F64(x)],
        )
    };
    assert_eq!(u(-1.0), Err(Trap::IntegerOverflow));
    assert_eq!(u(4294967295.9).unwrap(), vec![Value::I32(-1)]);
}

#[test]
fn saturating_truncation() {
    let t = |x: f64| {
        exec(
            vec![ValType::F64],
            vec![ValType::I32],
            vec![LocalGet(0), I32TruncSatF64S],
            vec![Value::F64(x)],
        )
        .unwrap()
    };
    assert_eq!(t(f64::NAN), vec![Value::I32(0)]);
    assert_eq!(t(1e30), vec![Value::I32(i32::MAX)]);
    assert_eq!(t(-1e30), vec![Value::I32(i32::MIN)]);
    assert_eq!(t(-1.5), vec![Value::I32(-1)]);

    let u = exec(
        vec![ValType::F32],
        vec![ValType::I64],
        vec![LocalGet(0), I64TruncSatF32U],
        vec![Value::F32(-3.0)],
    )
    .unwrap();
    assert_eq!(u, vec![Value::I64(0)]);
}

#[test]
fn conversions_and_reinterpret() {
    assert_eq!(
        exec(vec![ValType::I64], vec![ValType::I32], vec![LocalGet(0), I32WrapI64], vec![Value::I64(0x1_0000_0001)]).unwrap(),
        vec![Value::I32(1)]
    );
    assert_eq!(
        exec(vec![ValType::I32], vec![ValType::I64], vec![LocalGet(0), I64ExtendI32U], vec![Value::I32(-1)]).unwrap(),
        vec![Value::I64(0xffff_ffff)]
    );
    assert_eq!(
        exec(vec![ValType::F32], vec![ValType::I32], vec![LocalGet(0), I32ReinterpretF32], vec![Value::F32(1.0)]).unwrap(),
        vec![Value::I32(0x3f80_0000)]
    );
    assert_eq!(
        exec(vec![ValType::I32], vec![ValType::F32], vec![LocalGet(0), F32ReinterpretI32], vec![Value::I32(0x4000_0000)]).unwrap(),
        vec![Value::F32(2.0)]
    );
    assert_eq!(
        exec(vec![ValType::I64], vec![ValType::F64], vec![LocalGet(0), F64ReinterpretI64], vec![Value::I64(0x3ff0_0000_0000_0000)]).unwrap(),
        vec![Value::F64(1.0)]
    );
    assert_eq!(
        exec(vec![ValType::I64], vec![ValType::F64], vec![LocalGet(0), F64ConvertI64U], vec![Value::I64(-1)]).unwrap(),
        vec![Value::F64(18446744073709551615.0)]
    );
}

#[test]
fn sign_extension_ops() {
    assert_eq!(
        exec(vec![ValType::I32], vec![ValType::I32], vec![LocalGet(0), I32Extend8S], vec![Value::I32(0x80)]).unwrap(),
        vec![Value::I32(-128)]
    );
    assert_eq!(
        exec(vec![ValType::I64], vec![ValType::I64], vec![LocalGet(0), I64Extend32S], vec![Value::I64(0x8000_0000)]).unwrap(),
        vec![Value::I64(-2147483648)]
    );
}

#[test]
fn narrowing_loads() {
    let mut m = Module::default();
    m.types.push(FuncType { params: vec![], results: vec![ValType::I64] });
    m.memories.push(MemoryType { limits: WLimits::new(1, None) });
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![],
            body: vec![
                I32Const(0),
                I64Const(0x8877_6655_4433_2211u64 as i64),
                Instr::Store { kind: vela_core::instr::StoreKind::I64, mem: 0, arg: MemArg::new(3, 0) },
                I32Const(0),
                Load { kind: LoadKind::I64x8S, mem: 0, arg: MemArg::new(0, 0) },
                End,
            ],
        },
    });
    m.exports.push(Export { name: "f".into(), desc: ExportDesc::Func(0) });
    let mut store = Store::new();
    let inst = instantiate(&mut store, m, &Registry::new()).unwrap();
    let out = invoke(&mut store, 0, vec![], Limits::default()).unwrap();
    assert_eq!(out, vec![Value::I64(0x11)]);
    let _ = inst;
}

#[test]
fn float_comparisons_with_nan() {
    let cmp = |op: Instr, a: f64, b: f64| {
        exec(
            vec![ValType::F64, ValType::F64],
            vec![ValType::I32],
            vec![LocalGet(0), LocalGet(1), op],
            vec![Value::F64(a), Value::F64(b)],
        )
        .unwrap()
    };
    assert_eq!(cmp(F64Eq, f64::NAN, f64::NAN), vec![Value::I32(0)]);
    assert_eq!(cmp(F64Ne, f64::NAN, f64::NAN), vec![Value::I32(1)]);
    assert_eq!(cmp(F64Lt, f64::NAN, 1.0), vec![Value::I32(0)]);
    assert_eq!(cmp(F64Ge, 1.0, 1.0), vec![Value::I32(1)]);
}

#[test]
fn step_machine_exposes_state() {
    let mut m = Module::default();
    m.types.push(FuncType { params: vec![ValType::I32], results: vec![ValType::I32] });
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![],
            body: vec![LocalGet(0), I32Const(2), I32Mul, End],
        },
    });
    let mut store = Store::new();
    let inst = instantiate(&mut store, m, &Registry::new()).unwrap();
    let addr = store.instances[inst as usize].func_addrs[0];
    let machine = vela_core::runtime::Machine::new(Limits::default());
    let mut st = machine.start(&mut store, addr, vec![Value::I32(21)]).unwrap();
    assert_eq!(st.steps, 0);
    let mut guard = 0;
    while !st.is_finished() {
        machine.step(&mut store, &mut st);
        guard += 1;
        assert!(guard < 100);
    }
    assert_eq!(st.results, vec![Value::I32(42)]);
    assert!(st.steps >= 4);
}
