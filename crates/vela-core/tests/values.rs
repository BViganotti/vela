//! Runtime value formatting, trap codes and deterministic hashing.

use vela_core::runtime::{Hasher, HostEnv, Trap, Value};

#[test]
fn trap_codes_are_stable_and_nonempty() {
    let traps = vec![
        Trap::Unreachable,
        Trap::MemoryOutOfBounds,
        Trap::TableOutOfBounds,
        Trap::IndirectCallTypeMismatch,
        Trap::UninitializedElement,
        Trap::UndefinedElement,
        Trap::IntegerDivideByZero,
        Trap::IntegerOverflow,
        Trap::InvalidConversionToInteger,
        Trap::StackOverflow,
        Trap::OutOfFuel,
        Trap::DataSegmentDropped,
        Trap::ElemSegmentDropped,
        Trap::UnalignedAtomic,
        Trap::Host("boom".into()),
        Trap::LimitExceeded("stack"),
        Trap::TypeMismatch("i32"),
    ];
    for t in traps {
        assert!(!t.code().is_empty());
        assert!(!t.to_string().is_empty());
        // Traps implement Error for ergonomic embedding.
        let _: &dyn std::error::Error = &t;
    }
    assert_eq!(
        Trap::MemoryOutOfBounds.to_string(),
        "out of bounds memory access"
    );
    assert_eq!(Trap::Host("x".into()).to_string(), "host error: x");
    assert_eq!(
        Trap::LimitExceeded("fuel").to_string(),
        "resource limit exceeded: fuel"
    );
}

#[test]
fn value_display_is_explicit() {
    assert_eq!(Value::I32(-1).to_string(), "-1");
    assert_eq!(Value::I64(2).to_string(), "2");
    assert_eq!(Value::FuncRef(None).to_string(), "null");
    assert_eq!(Value::FuncRef(Some(7)).to_string(), "funcref:7");
    assert_eq!(Value::ExternRef(Some(3)).to_string(), "externref:3");
    assert_eq!(Value::F64(1.5).to_string(), "1.5");
}

#[test]
fn hasher_matches_fnv1a_golden() {
    // FNV-1a 64-bit of the single byte 'a'.
    let mut h = Hasher::new();
    h.write_u8(b'a');
    assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
}

#[test]
fn value_hashes_are_tag_separated() {
    let hash = |v: Value| {
        let mut h = Hasher::new();
        h.write_value(&v);
        h.finish()
    };
    assert_ne!(hash(Value::I32(1)), hash(Value::I64(1)));
    assert_ne!(hash(Value::F32(1.0)), hash(Value::I32(1)));
    assert_ne!(hash(Value::FuncRef(None)), hash(Value::ExternRef(None)));
    assert_eq!(hash(Value::F32(-0.0)), hash(Value::F32(-0.0)));
    assert_ne!(hash(Value::F32(0.0)), hash(Value::F32(-0.0)));
}

#[test]
fn host_environment_is_deterministic() {
    let mut a = HostEnv::new(42);
    let mut b = HostEnv::new(42);
    let mut c = HostEnv::new(43);
    let seq_a: Vec<u64> = (0..8).map(|_| a.random_u64()).collect();
    let seq_b: Vec<u64> = (0..8).map(|_| b.random_u64()).collect();
    let seq_c: Vec<u64> = (0..8).map(|_| c.random_u64()).collect();
    assert_eq!(seq_a, seq_b);
    assert_ne!(seq_a, seq_c);
}

#[test]
fn host_clock_advances_monotonically() {
    let mut env = HostEnv::new(0);
    assert_eq!(env.now_millis(), 1);
    assert_eq!(env.now_millis(), 2);
    assert_eq!(env.clock(), 2);
    assert_eq!(env.call_count(), 2);
    let r = env.random_u64();
    assert_eq!(env.call_count(), 3);
    let _ = r;
}
