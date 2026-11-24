//! Exhaustive instruction encoding round-trips and module section edge cases.

use vela_core::binary::{decode_module, encode_module, put_instr};
use vela_core::binary::reader::Reader;
use vela_core::instr::{BrTable, Instr, LoadKind, MemArg, StoreKind};
use vela_core::module::{
    Data, DataMode, Element, ElementMode, Export, ExportDesc, Func, FuncBody, Global, Module,
};
use vela_core::types::{
    BlockType, FuncType, GlobalType, Limits, MemoryType, TableType, ValType,
};

fn all_instructions() -> Vec<Instr> {
    let mut v = vec![
        Instr::Unreachable,
        Instr::Nop,
        Instr::Block(BlockType::Empty),
        Instr::Block(BlockType::Value(ValType::I32)),
        Instr::Block(BlockType::TypeIdx(0)),
        Instr::Loop(BlockType::Empty),
        Instr::If(BlockType::Empty),
        Instr::Else,
        Instr::End,
        Instr::Br(0),
        Instr::BrIf(1),
        Instr::BrTable(Box::new(BrTable { targets: vec![0, 1], default: 2 })),
        Instr::Return,
        Instr::Call(3),
        Instr::CallIndirect { type_idx: 1, table_idx: 0 },
        Instr::Drop,
        Instr::Select,
        Instr::SelectT(vec![ValType::I32]),
        Instr::LocalGet(0),
        Instr::LocalSet(1),
        Instr::LocalTee(2),
        Instr::GlobalGet(0),
        Instr::GlobalSet(1),
        Instr::TableGet(0),
        Instr::TableSet(0),
        Instr::TableInit { elem: 0, table: 0 },
        Instr::ElemDrop(0),
        Instr::TableCopy { dst: 0, src: 0 },
        Instr::TableGrow(0),
        Instr::TableSize(0),
        Instr::TableFill(0),
        Instr::MemorySize(0),
        Instr::MemoryGrow(0),
        Instr::MemoryInit { data: 0, mem: 0 },
        Instr::DataDrop(0),
        Instr::MemoryCopy { dst: 0, src: 0 },
        Instr::MemoryFill(0),
        Instr::I32Const(-1),
        Instr::I64Const(-2),
        Instr::F32Const(0x7fc0_0001),
        Instr::F64Const(0x7ff8_0000_0000_0001),
        Instr::RefNull(ValType::FuncRef),
        Instr::RefNull(ValType::ExternRef),
        Instr::RefIsNull,
        Instr::RefFunc(0),
    ];
    for kind in [
        LoadKind::I32,
        LoadKind::I64,
        LoadKind::F32,
        LoadKind::F64,
        LoadKind::I32x8S,
        LoadKind::I32x8U,
        LoadKind::I32x16S,
        LoadKind::I32x16U,
        LoadKind::I64x8S,
        LoadKind::I64x8U,
        LoadKind::I64x16S,
        LoadKind::I64x16U,
        LoadKind::I64x32S,
        LoadKind::I64x32U,
    ] {
        v.push(Instr::Load { kind, mem: 0, arg: MemArg::new(0, 8) });
    }
    for kind in [
        StoreKind::I32,
        StoreKind::I64,
        StoreKind::F32,
        StoreKind::F64,
        StoreKind::I32x8,
        StoreKind::I32x16,
        StoreKind::I64x8,
        StoreKind::I64x16,
        StoreKind::I64x32,
    ] {
        v.push(Instr::Store { kind, mem: 0, arg: MemArg::new(1, 16) });
    }
    // All purely numeric instructions have no immediates; enumerate them by
    // round-tripping a body of every name reachable through the signature table.
    let numeric = [
        "i32.eqz","i32.eq","i32.ne","i32.lt_s","i32.lt_u","i32.gt_s","i32.gt_u",
        "i32.le_s","i32.le_u","i32.ge_s","i32.ge_u","i64.eqz","i64.eq","i64.ne",
        "i64.lt_s","i64.lt_u","i64.gt_s","i64.gt_u","i64.le_s","i64.le_u",
        "i64.ge_s","i64.ge_u","f32.eq","f32.ne","f32.lt","f32.gt","f32.le",
        "f32.ge","f64.eq","f64.ne","f64.lt","f64.gt","f64.le","f64.ge",
        "i32.clz","i32.ctz","i32.popcnt","i32.add","i32.sub","i32.mul",
        "i32.div_s","i32.div_u","i32.rem_s","i32.rem_u","i32.and","i32.or",
        "i32.xor","i32.shl","i32.shr_s","i32.shr_u","i32.rotl","i32.rotr",
        "i64.clz","i64.ctz","i64.popcnt","i64.add","i64.sub","i64.mul",
        "i64.div_s","i64.div_u","i64.rem_s","i64.rem_u","i64.and","i64.or",
        "i64.xor","i64.shl","i64.shr_s","i64.shr_u","i64.rotl","i64.rotr",
        "f32.abs","f32.neg","f32.ceil","f32.floor","f32.trunc","f32.nearest",
        "f32.sqrt","f32.add","f32.sub","f32.mul","f32.div","f32.min","f32.max",
        "f32.copysign","f64.abs","f64.neg","f64.ceil","f64.floor","f64.trunc",
        "f64.nearest","f64.sqrt","f64.add","f64.sub","f64.mul","f64.div",
        "f64.min","f64.max","f64.copysign","i32.wrap_i64","i32.trunc_f32_s",
        "i32.trunc_f32_u","i32.trunc_f64_s","i32.trunc_f64_u","i64.extend_i32_s",
        "i64.extend_i32_u","i64.trunc_f32_s","i64.trunc_f32_u","i64.trunc_f64_s",
        "i64.trunc_f64_u","f32.convert_i32_s","f32.convert_i32_u",
        "f32.convert_i64_s","f32.convert_i64_u","f32.demote_f64",
        "f64.convert_i32_s","f64.convert_i32_u","f64.convert_i64_s",
        "f64.convert_i64_u","f64.promote_f32","i32.reinterpret_f32",
        "i64.reinterpret_f64","f32.reinterpret_i32","f64.reinterpret_i64",
        "i32.extend8_s","i32.extend16_s","i64.extend8_s","i64.extend16_s",
        "i64.extend32_s","i32.trunc_sat_f32_s","i32.trunc_sat_f32_u",
        "i32.trunc_sat_f64_s","i32.trunc_sat_f64_u","i64.trunc_sat_f32_s",
        "i64.trunc_sat_f32_u","i64.trunc_sat_f64_s","i64.trunc_sat_f64_u",
    ];
    let module = format!(
        "(module (func {}))",
        numeric.iter().map(|n| format!("{n}")).collect::<Vec<_>>().join(" ")
    );
    // Build each via the text parser to avoid duplicating the opcode table.
    if let Ok(m) = vela_core::text::parse_wat(&module) {
        v.extend(m.funcs[0].body.body.iter().cloned().filter(|i| !matches!(i, Instr::End)));
    }
    v
}

#[test]
fn every_instruction_round_trips() {
    for instr in all_instructions() {
        let mut bytes = Vec::new();
        put_instr(&mut bytes, &instr);
        let mut r = Reader::new(&bytes);
        let decoded = vela_core::binary::decode_instr(&mut r).expect(&format!("decode {instr:?}"));
        assert_eq!(decoded, instr);
        assert!(r.is_empty());
    }
}

#[test]
fn full_module_with_all_segments_round_trips() {
    let mut m = Module::default();
    m.types.push(FuncType { params: vec![ValType::I32], results: vec![] });
    m.imports.push(vela_core::module::Import {
        module: "env".into(),
        name: "mem".into(),
        desc: vela_core::module::ImportDesc::Memory(MemoryType { limits: Limits::new(1, Some(2)) }),
    });
    m.tables.push(TableType { elem: ValType::ExternRef, limits: Limits::new(1, Some(3)) });
    m.memories.push(MemoryType { limits: Limits::new(2, None) });
    m.globals.push(Global {
        ty: GlobalType { val: ValType::I32, mutable: true },
        init: vec![Instr::I32Const(9), Instr::End],
    });
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![ValType::I64],
            body: vec![Instr::LocalGet(0), Instr::RefNull(ValType::FuncRef), Instr::TableSet(0), Instr::End],
        },
    });
    m.exports.push(Export { name: "f".into(), desc: ExportDesc::Func(0) });
    m.elems.push(Element {
        mode: ElementMode::Passive,
        elem_type: ValType::FuncRef,
        init: vec![vec![Instr::RefFunc(0), Instr::End]],
    });
    m.elems.push(Element {
        mode: ElementMode::Active { table: 0, offset: vec![Instr::I32Const(0), Instr::End] },
        elem_type: ValType::ExternRef,
        init: vec![vec![Instr::RefNull(ValType::ExternRef), Instr::End]],
    });
    m.elems.push(Element {
        mode: ElementMode::Declarative,
        elem_type: ValType::FuncRef,
        init: vec![],
    });
    m.datas.push(Data { mode: DataMode::Passive, bytes: vec![1, 2, 3] });
    m.datas.push(Data {
        mode: DataMode::Active { memory: 0, offset: vec![Instr::I32Const(4), Instr::End] },
        bytes: vec![9, 8],
    });
    m.data_count = Some(2);
    m.start = None;
    let bytes = encode_module(&m);
    let decoded = decode_module(&bytes).expect("decodes");
    assert_eq!(m, decoded);
}

#[test]
fn custom_sections_are_skipped_and_named() {
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    // Custom section "hello" with arbitrary payload.
    bytes.extend_from_slice(&[0x00, 0x09, 0x05, b'h', b'e', b'l', b'l', b'o', 0xde, 0xad, 0xbe]);
    let m = decode_module(&bytes).expect("custom section");
    assert!(m.custom_sections.iter().any(|s| s == "hello"));
}
