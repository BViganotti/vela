//! Instantiation: import matching, allocation caps, rollback and start traps.

use vela_core::module::{Export, ExportDesc, Func, FuncBody, Import, ImportDesc, Module};
use vela_core::runtime::store::limits_match;
use vela_core::runtime::{instantiate, FuncKind, InstantiateError, Registry, Store, Trap};
use vela_core::text::parse_wat;
use vela_core::types::{FuncType, Limits, MemoryType, TableType, ValType};

fn host_module(import_ty: FuncType, host_ty: FuncType) -> (Module, Registry, Store) {
    let mut m = Module::default();
    m.types.push(import_ty);
    m.imports.push(Import {
        module: "env".into(),
        name: "f".into(),
        desc: ImportDesc::Func(0),
    });
    m.types.push(FuncType::default());
    m.funcs.push(Func {
        type_idx: 1,
        body: FuncBody {
            locals: vec![],
            body: vec![vela_core::instr::Instr::End],
        },
    });
    let mut registry = Registry::new();
    registry.define_host(
        "env",
        "f",
        host_ty,
        std::rc::Rc::new(
            |_: &mut vela_core::runtime::HostEnv, _: &[vela_core::runtime::Value]| Ok(vec![]),
        ),
    );
    let mut store = Store::new();
    registry.install_hosts(&mut store);
    (m, registry, store)
}

#[test]
fn missing_import_is_reported() {
    let mut m = Module::default();
    m.types.push(FuncType::default());
    m.imports.push(Import {
        module: "env".into(),
        name: "nope".into(),
        desc: ImportDesc::Func(0),
    });
    let mut store = Store::new();
    let err = instantiate(&mut store, m, &Registry::new()).unwrap_err();
    assert!(matches!(err, InstantiateError::Import { .. }), "{err}");
    assert!(store.instances.is_empty());
}

#[test]
fn import_type_mismatch_is_rejected() {
    let expected = FuncType {
        params: vec![ValType::I32],
        results: vec![],
    };
    let actual = FuncType {
        params: vec![ValType::I64],
        results: vec![],
    };
    let (m, registry, mut store) = host_module(expected, actual);
    let before = store.funcs.len();
    let err = instantiate(&mut store, m, &registry).unwrap_err();
    assert!(matches!(err, InstantiateError::Import { .. }), "{err}");
    assert_eq!(
        store.funcs.len(),
        before,
        "no allocations on import failure"
    );
}

#[test]
fn memory_cap_is_enforced() {
    let mut m = Module::default();
    m.memories.push(MemoryType {
        limits: Limits::new(64, None),
    });
    let mut store = Store::new();
    store.max_memory_bytes = 1024 * 1024; // one MiB
    let err = instantiate(&mut store, m, &Registry::new()).unwrap_err();
    assert!(matches!(err, InstantiateError::Limit(_)), "{err}");
    assert!(store.memories.is_empty());
}

#[test]
fn table_cap_is_enforced() {
    let mut m = Module::default();
    m.tables.push(TableType {
        elem: ValType::FuncRef,
        limits: Limits::new(100, None),
    });
    let mut store = Store::new();
    store.max_table_entries = 10;
    let err = instantiate(&mut store, m, &Registry::new()).unwrap_err();
    assert!(matches!(err, InstantiateError::Limit(_)), "{err}");
    assert!(store.tables.is_empty());
}

#[test]
fn start_trap_rolls_back_allocations() {
    let m = parse_wat(
        r#"(module
             (memory 1)
             (global i32 (i32.const 1))
             (func $bad unreachable)
             (start $bad))"#,
    )
    .unwrap();
    let mut store = Store::new();
    let err = instantiate(&mut store, m, &Registry::new()).unwrap_err();
    assert!(
        matches!(err, InstantiateError::Trap(Trap::Unreachable)),
        "{err}"
    );
    assert!(store.funcs.is_empty());
    assert!(store.memories.is_empty());
    assert!(store.globals.is_empty());
    assert!(store.instances.is_empty());
}

#[test]
fn import_limits_matching() {
    assert!(limits_match(Limits::new(1, None), Limits::new(2, None)));
    assert!(limits_match(
        Limits::new(1, Some(4)),
        Limits::new(2, Some(4))
    ));
    assert!(!limits_match(Limits::new(2, None), Limits::new(1, None)));
    assert!(!limits_match(Limits::new(1, Some(4)), Limits::new(2, None)));
    assert!(!limits_match(
        Limits::new(1, Some(4)),
        Limits::new(2, Some(5))
    ));
}

#[test]
fn exported_and_imported_function_addresses_resolve() {
    let mut m = parse_wat(r#"(module (func (export "f") (result i32) i32.const 3))"#).unwrap();
    m.exports.push(Export {
        name: "g".into(),
        desc: ExportDesc::Func(0),
    });
    let mut store = Store::new();
    let inst = instantiate(&mut store, m, &Registry::new()).unwrap();
    let inst_ref = &store.instances[inst as usize];
    let a = inst_ref.exports["f"];
    let b = inst_ref.exports["g"];
    assert_eq!(a, b);
    match a {
        vela_core::runtime::ExternVal::Func(addr) => {
            let f = store.func(addr).unwrap();
            assert!(matches!(f.kind, FuncKind::Wasm { .. }));
            assert_eq!(f.ty.results, vec![ValType::I32]);
        }
        _ => panic!("not a function"),
    }
}
