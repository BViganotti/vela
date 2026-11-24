//! Store state hashing: stable, sensitive and snapshot-isolated.

use vela_core::runtime::global::GlobalInst;
use vela_core::runtime::memory::MemoryInst;
use vela_core::runtime::table::TableInst;
use vela_core::runtime::Store;
use vela_core::types::{GlobalType, Limits, ValType};

fn seeded() -> Store {
    let mut store = Store::new();
    store.alloc_memory(MemoryInst::new(Limits::new(1, Some(2))));
    store.alloc_table(TableInst::new(ValType::FuncRef, Limits::new(2, Some(4))));
    store.alloc_global(GlobalInst::new(
        GlobalType { val: ValType::I32, mutable: true },
        vela_core::runtime::Value::I32(5),
    ));
    store
}

#[test]
fn hash_is_stable_across_clones() {
    let store = seeded();
    let h = store.state_hash();
    let clone = store.clone();
    assert_eq!(clone.state_hash(), h);
    assert_eq!(store.state_hash(), h);
}

#[test]
fn memory_bytes_change_the_hash() {
    let store = seeded();
    let h = store.state_hash();
    let mut other = store.clone();
    other.memory_mut(0).unwrap().write(0, 0, b"x").unwrap();
    assert_ne!(other.state_hash(), h);
}

#[test]
fn globals_and_tables_change_the_hash() {
    let store = seeded();
    let h = store.state_hash();
    let mut a = store.clone();
    a.global_mut(0).unwrap().set(vela_core::runtime::Value::I32(6)).unwrap();
    assert_ne!(a.state_hash(), h);

    let mut b = store.clone();
    b.table_mut(0)
        .unwrap()
        .set(0, vela_core::runtime::Value::FuncRef(Some(1)))
        .unwrap();
    assert_ne!(b.state_hash(), h);

    let mut c = store.clone();
    c.table_mut(0)
        .unwrap()
        .grow(1, vela_core::runtime::Value::FuncRef(None));
    assert_ne!(c.state_hash(), h);
}

#[test]
fn snapshots_are_independent() {
    let store = seeded();
    let mut copy = store.clone();
    copy.memory_mut(0).unwrap().fill(0, 7, 8).unwrap();
    assert!(store.memory(0).unwrap().data()[..8].iter().all(|b| *b == 0));
    assert!(copy.memory(0).unwrap().data()[..8].iter().all(|b| *b == 7));
}

#[test]
fn host_state_contributes_to_the_hash() {
    let mut store = seeded();
    let h = store.state_hash();
    let _ = store.host.now_millis();
    assert_ne!(store.state_hash(), h);
}
