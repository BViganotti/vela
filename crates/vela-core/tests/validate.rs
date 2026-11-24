//! Validator tests covering stack typing, control flow, limits and segments.

use vela_core::instr::{Instr, LoadKind, MemArg};
use vela_core::module::{Data, DataMode, Export, ExportDesc, Func, FuncBody, Global, Module};
use vela_core::types::{FuncType, GlobalType, Limits, MemoryType, TableType, ValType};
use vela_core::validate::validate_module;
use Instr::*;

fn empty_module() -> Module {
    Module::default()
}

/// Add a function with the given signature and body (without the final end).
fn add_func(
    m: &mut Module,
    params: Vec<ValType>,
    results: Vec<ValType>,
    locals: Vec<ValType>,
    body: Vec<Instr>,
) -> u32 {
    let type_idx = m.types.len() as u32;
    m.types.push(FuncType { params, results });
    let mut b = body;
    b.push(End);
    m.funcs.push(Func {
        type_idx,
        body: FuncBody { locals, body: b },
    });
    m.funcs.len() as u32 - 1
}

#[test]
fn accepts_valid_add() {
    let mut m = empty_module();
    add_func(
        &mut m,
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
        vec![],
        vec![LocalGet(0), LocalGet(1), I32Add],
    );
    m.exports.push(Export {
        name: "add".into(),
        desc: ExportDesc::Func(0),
    });
    validate_module(&m).unwrap();
}

#[test]
fn rejects_wrong_result_type() {
    let mut m = empty_module();
    add_func(&mut m, vec![], vec![ValType::I32], vec![], vec![]);
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_stack_underflow() {
    let mut m = empty_module();
    add_func(&mut m, vec![], vec![ValType::I32], vec![], vec![I32Add]);
    assert!(validate_module(&m).is_err());
}

#[test]
fn unbounded_polymorphism_after_unreachable() {
    let mut m = empty_module();
    add_func(
        &mut m,
        vec![],
        vec![ValType::I32],
        vec![],
        vec![Unreachable, I32Add],
    );
    validate_module(&m).unwrap();
}

#[test]
fn rejects_stray_value_at_block_end() {
    let mut m = empty_module();
    add_func(
        &mut m,
        vec![],
        vec![],
        vec![],
        vec![Block(vela_core::types::BlockType::Empty), I32Const(1), End],
    );
    assert!(validate_module(&m).is_err());
}

#[test]
fn accepts_block_with_result() {
    let mut m = empty_module();
    add_func(
        &mut m,
        vec![],
        vec![ValType::I32],
        vec![],
        vec![
            Block(vela_core::types::BlockType::Value(ValType::I32)),
            I32Const(7),
            End,
        ],
    );
    validate_module(&m).unwrap();
}

#[test]
fn rejects_unknown_branch_label() {
    let mut m = empty_module();
    add_func(&mut m, vec![], vec![], vec![], vec![Br(5)]);
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_br_table_mismatch() {
    let mut m = empty_module();
    add_func(
        &mut m,
        vec![],
        vec![],
        vec![],
        vec![
            Block(vela_core::types::BlockType::Empty),
            Block(vela_core::types::BlockType::Value(ValType::I32)),
            I32Const(0),
            I32Const(0),
            BrTable(Box::new(vela_core::instr::BrTable {
                targets: vec![0],
                default: 1,
            })),
            End,
            Drop,
            End,
        ],
    );
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_if_without_else_with_mismatched_types() {
    let mut m = empty_module();
    add_func(
        &mut m,
        vec![ValType::I32],
        vec![ValType::I32],
        vec![],
        vec![
            LocalGet(0),
            If(vela_core::types::BlockType::Empty),
            I32Const(1),
            End,
        ],
    );
    assert!(validate_module(&m).is_err());
}

#[test]
fn accepts_if_with_else() {
    let mut m = empty_module();
    add_func(
        &mut m,
        vec![ValType::I32],
        vec![ValType::I32],
        vec![],
        vec![
            LocalGet(0),
            If(vela_core::types::BlockType::Value(ValType::I32)),
            I32Const(1),
            Else,
            I32Const(2),
            End,
        ],
    );
    validate_module(&m).unwrap();
}

#[test]
fn rejects_bad_call_arguments() {
    let mut m = empty_module();
    add_func(&mut m, vec![ValType::I64], vec![], vec![], vec![]);
    add_func(
        &mut m,
        vec![],
        vec![],
        vec![],
        vec![I32Const(1), Call(0), Drop],
    );
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_missing_memory_for_load() {
    let mut m = empty_module();
    add_func(
        &mut m,
        vec![],
        vec![ValType::I32],
        vec![],
        vec![
            I32Const(0),
            Load {
                kind: LoadKind::I32,
                mem: 0,
                arg: MemArg::new(2, 0),
            },
        ],
    );
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_overaligned_load() {
    let mut m = empty_module();
    m.memories.push(MemoryType {
        limits: Limits::new(1, None),
    });
    add_func(
        &mut m,
        vec![],
        vec![ValType::I32],
        vec![],
        vec![
            I32Const(0),
            Load {
                kind: LoadKind::I32,
                mem: 0,
                arg: MemArg::new(3, 0),
            },
        ],
    );
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_immutable_global_set() {
    let mut m = empty_module();
    m.globals.push(Global {
        ty: GlobalType {
            val: ValType::I32,
            mutable: false,
        },
        init: vec![I32Const(0), End],
    });
    add_func(
        &mut m,
        vec![],
        vec![],
        vec![],
        vec![I32Const(1), GlobalSet(0)],
    );
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_duplicate_exports() {
    let mut m = empty_module();
    add_func(&mut m, vec![], vec![], vec![], vec![]);
    m.exports.push(Export {
        name: "x".into(),
        desc: ExportDesc::Func(0),
    });
    m.exports.push(Export {
        name: "x".into(),
        desc: ExportDesc::Func(0),
    });
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_bad_start_signature() {
    let mut m = empty_module();
    add_func(&mut m, vec![ValType::I32], vec![], vec![], vec![]);
    m.start = Some(0);
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_non_imported_global_in_const_expr() {
    let mut m = empty_module();
    m.globals.push(Global {
        ty: GlobalType {
            val: ValType::I32,
            mutable: false,
        },
        init: vec![I32Const(0), End],
    });
    m.globals.push(Global {
        ty: GlobalType {
            val: ValType::I32,
            mutable: false,
        },
        init: vec![GlobalGet(0), End],
    });
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_active_data_without_memory() {
    let mut m = empty_module();
    m.datas.push(Data {
        mode: DataMode::Active {
            memory: 0,
            offset: vec![I32Const(0), End],
        },
        bytes: vec![1, 2, 3],
    });
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_undeclared_ref_func() {
    let mut m = empty_module();
    add_func(
        &mut m,
        vec![],
        vec![ValType::FuncRef],
        vec![],
        vec![RefFunc(0)],
    );
    assert!(validate_module(&m).is_err());
    // Declaring the function through an export satisfies the refs set.
    let mut m2 = empty_module();
    add_func(
        &mut m2,
        vec![],
        vec![ValType::FuncRef],
        vec![],
        vec![RefFunc(0)],
    );
    m2.exports.push(Export {
        name: "f".into(),
        desc: ExportDesc::Func(0),
    });
    validate_module(&m2).unwrap();
}

#[test]
fn rejects_memory_init_without_data_count() {
    let mut m = empty_module();
    m.memories.push(MemoryType {
        limits: Limits::new(1, None),
    });
    m.datas.push(Data {
        mode: DataMode::Passive,
        bytes: vec![0],
    });
    add_func(
        &mut m,
        vec![],
        vec![],
        vec![],
        vec![
            I32Const(0),
            I32Const(0),
            I32Const(1),
            MemoryInit { data: 0, mem: 0 },
        ],
    );
    assert!(validate_module(&m).is_err());
    m.data_count = Some(1);
    validate_module(&m).unwrap();
}

#[test]
fn rejects_table_copy_type_mismatch() {
    let mut m = empty_module();
    m.tables.push(TableType {
        elem: ValType::FuncRef,
        limits: Limits::new(1, None),
    });
    m.tables.push(TableType {
        elem: ValType::ExternRef,
        limits: Limits::new(1, None),
    });
    add_func(
        &mut m,
        vec![],
        vec![],
        vec![],
        vec![
            I32Const(0),
            I32Const(0),
            I32Const(1),
            TableCopy { dst: 0, src: 1 },
        ],
    );
    assert!(validate_module(&m).is_err());
}

#[test]
fn rejects_two_memories() {
    let mut m = empty_module();
    m.memories.push(MemoryType {
        limits: Limits::new(1, None),
    });
    let type_idx = m.types.len() as u32;
    m.types.push(FuncType::default());
    m.imports.push(vela_core::module::Import {
        module: "env".into(),
        name: "mem".into(),
        desc: vela_core::module::ImportDesc::Memory(MemoryType {
            limits: Limits::new(1, None),
        }),
    });
    m.funcs.push(Func {
        type_idx,
        body: FuncBody {
            locals: vec![],
            body: vec![End],
        },
    });
    assert!(validate_module(&m).is_err());
}
