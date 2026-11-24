//! Additional validation rules: tables, segments, exports, start and bodies.

use vela_core::instr::Instr;
use vela_core::module::{
    Data, DataMode, Element, ElementMode, Export, ExportDesc, Func, FuncBody, Global, Import,
    ImportDesc, Module,
};
use vela_core::types::{
    FuncType, GlobalType, Limits, MemoryType, TableType, ValType, MAX_PAGES,
};
use vela_core::validate::validate_module;

fn func(m: &mut Module, body: Vec<Instr>) -> u32 {
    let ty = if m.types.is_empty() {
        m.types.push(FuncType::default());
        0
    } else {
        // Type 0 must be [] -> []; callers that need another type add it.
        let explicit = m.types.len() as u32;
        m.types.push(FuncType::default());
        explicit
    };
    m.funcs.push(Func { type_idx: ty, body: FuncBody { locals: vec![], body } });
    m.funcs.len() as u32 - 1
}

#[test]
fn call_indirect_requires_a_funcref_table() {
    let mut m = Module::default();
    m.types.push(FuncType::default());
    m.tables.push(TableType { elem: ValType::ExternRef, limits: Limits::new(1, None) });
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![],
            body: vec![
                Instr::I32Const(0),
                Instr::CallIndirect { type_idx: 0, table_idx: 0 },
                Instr::End,
            ],
        },
    });
    let e = validate_module(&m).unwrap_err();
    assert!(e.message.contains("funcref"), "{e}");
}

#[test]
fn call_indirect_requires_a_known_type() {
    let mut m = Module::default();
    m.types.push(FuncType::default());
    m.tables.push(TableType { elem: ValType::FuncRef, limits: Limits::new(1, None) });
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody {
            locals: vec![],
            body: vec![
                Instr::I32Const(0),
                Instr::CallIndirect { type_idx: 9, table_idx: 0 },
                Instr::End,
            ],
        },
    });
    assert!(validate_module(&m).is_err());
}

#[test]
fn element_type_must_match_table() {
    let mut m = Module::default();
    func(&mut m, vec![Instr::End]);
    m.tables.push(TableType { elem: ValType::ExternRef, limits: Limits::new(1, None) });
    m.elems.push(Element {
        mode: ElementMode::Active { table: 0, offset: vec![Instr::I32Const(0), Instr::End] },
        elem_type: ValType::FuncRef,
        init: vec![vec![Instr::RefFunc(0), Instr::End]],
    });
    let e = validate_module(&m).unwrap_err();
    assert!(e.message.contains("type mismatch"), "{e}");
}

#[test]
fn data_count_must_match_segments() {
    let mut m = Module::default();
    m.data_count = Some(2);
    m.datas.push(Data { mode: DataMode::Passive, bytes: vec![1] });
    assert!(validate_module(&m).is_err());
}

#[test]
fn start_must_exist_and_have_empty_signature() {
    let mut m = Module::default();
    m.start = Some(3);
    assert!(validate_module(&m).is_err());

    let mut m = Module::default();
    m.types.push(FuncType { params: vec![ValType::I32], results: vec![] });
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody { locals: vec![], body: vec![Instr::End] },
    });
    m.start = Some(0);
    assert!(validate_module(&m).is_err());
}

#[test]
fn exports_and_imports_must_reference_valid_indices() {
    let mut m = Module::default();
    m.exports.push(Export { name: "x".into(), desc: ExportDesc::Func(9) });
    assert!(validate_module(&m).is_err());

    let mut m = Module::default();
    m.imports.push(Import { module: "a".into(), name: "b".into(), desc: ImportDesc::Func(4) });
    assert!(validate_module(&m).is_err());
}

#[test]
fn limits_and_global_initializers_are_checked() {
    let mut m = Module::default();
    m.memories.push(MemoryType { limits: Limits::new(MAX_PAGES + 1, None) });
    assert!(validate_module(&m).is_err());

    let mut m = Module::default();
    m.globals.push(Global {
        ty: GlobalType { val: ValType::I32, mutable: false },
        init: vec![Instr::I64Const(0), Instr::End],
    });
    assert!(validate_module(&m).is_err());

    let mut m = Module::default();
    m.globals.push(Global {
        ty: GlobalType { val: ValType::I32, mutable: false },
        init: vec![Instr::Nop, Instr::End],
    });
    assert!(validate_module(&m).is_err());
}

#[test]
fn bodies_must_end_exactly_once() {
    let mut m = Module::default();
    m.types.push(FuncType::default());
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody { locals: vec![], body: vec![Instr::End, Instr::Nop] },
    });
    assert!(validate_module(&m).is_err());

    let mut m = Module::default();
    m.types.push(FuncType::default());
    m.funcs.push(Func {
        type_idx: 0,
        body: FuncBody { locals: vec![], body: vec![Instr::Nop] },
    });
    assert!(validate_module(&m).is_err());
}

#[test]
fn tables_must_use_reference_types() {
    let mut m = Module::default();
    m.tables.push(TableType { elem: ValType::I32, limits: Limits::new(1, None) });
    assert!(validate_module(&m).is_err());
}

#[test]
fn undeclared_function_references_are_rejected() {
    let mut m = Module::default();
    func(&mut m, vec![Instr::End]);
    m.elems.push(Element {
        mode: ElementMode::Passive,
        elem_type: ValType::FuncRef,
        init: vec![vec![Instr::RefFunc(7), Instr::End]],
    });
    assert!(validate_module(&m).is_err());
}
