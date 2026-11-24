//! Runtime values, references and deterministic hashing.

use crate::types::ValType;
use std::fmt;

/// A runtime WebAssembly value.
///
/// Reference values are store addresses. `None` is the null reference.
#[derive(Clone, Copy, Debug)]
pub enum Value {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    FuncRef(Option<u32>),
    ExternRef(Option<u32>),
}

impl Value {
    pub fn ty(&self) -> ValType {
        match self {
            Value::I32(_) => ValType::I32,
            Value::I64(_) => ValType::I64,
            Value::F32(_) => ValType::F32,
            Value::F64(_) => ValType::F64,
            Value::FuncRef(_) => ValType::FuncRef,
            Value::ExternRef(_) => ValType::ExternRef,
        }
    }

    pub fn default_for(ty: ValType) -> Value {
        match ty {
            ValType::I32 => Value::I32(0),
            ValType::I64 => Value::I64(0),
            ValType::F32 => Value::F32(0.0),
            ValType::F64 => Value::F64(0.0),
            ValType::FuncRef => Value::FuncRef(None),
            ValType::ExternRef => Value::ExternRef(None),
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Value::I32(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::I64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Value::F32(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::F64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn is_null_ref(&self) -> bool {
        matches!(self, Value::FuncRef(None) | Value::ExternRef(None))
    }

    /// Deterministic FNV-1a hash over the exact value representation.
    pub fn hash_into(&self, h: &mut u64) {
        fn bytes(h: &mut u64, bytes: &[u8]) {
            for b in bytes {
                *h ^= *b as u64;
                *h = h.wrapping_mul(0x100000001b3);
            }
        }
        match self {
            Value::I32(v) => {
                *h ^= 1;
                bytes(h, &v.to_le_bytes());
            }
            Value::I64(v) => {
                *h ^= 2;
                bytes(h, &v.to_le_bytes());
            }
            Value::F32(v) => {
                *h ^= 3;
                bytes(h, &v.to_bits().to_le_bytes());
            }
            Value::F64(v) => {
                *h ^= 4;
                bytes(h, &v.to_bits().to_le_bytes());
            }
            Value::FuncRef(r) => {
                *h ^= 5;
                bytes(h, &r.unwrap_or(u32::MAX).to_le_bytes());
            }
            Value::ExternRef(r) => {
                *h ^= 6;
                bytes(h, &r.unwrap_or(u32::MAX).to_le_bytes());
            }
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::I32(a), Value::I32(b)) => a == b,
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::F32(a), Value::F32(b)) => a.to_bits() == b.to_bits(),
            (Value::F64(a), Value::F64(b)) => a.to_bits() == b.to_bits(),
            (Value::FuncRef(a), Value::FuncRef(b)) => a == b,
            (Value::ExternRef(a), Value::ExternRef(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::I32(v) => write!(f, "{v}"),
            Value::I64(v) => write!(f, "{v}"),
            Value::F32(v) => write!(f, "{v}"),
            Value::F64(v) => write!(f, "{v}"),
            Value::FuncRef(None) | Value::ExternRef(None) => f.write_str("null"),
            Value::FuncRef(Some(a)) => write!(f, "funcref:{a}"),
            Value::ExternRef(Some(a)) => write!(f, "externref:{a}"),
        }
    }
}

/// FNV-1a state used for trace and memory hashes.
#[derive(Clone, Copy, Debug)]
pub struct Hasher(pub u64);

impl Default for Hasher {
    fn default() -> Self {
        Hasher(0xcbf29ce484222325)
    }
}

impl Hasher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write_u8(&mut self, b: u8) {
        self.0 ^= b as u64;
        self.0 = self.0.wrapping_mul(0x100000001b3);
    }

    pub fn write_u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.write_u8(b);
        }
    }

    pub fn write_u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.write_u8(b);
        }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_u8(*b);
        }
    }

    pub fn write_value(&mut self, v: &Value) {
        v.hash_into(&mut self.0);
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_hashes_are_distinct_from_null_refs() {
        let mut a = Hasher::new();
        a.write_value(&Value::I32(0));
        let mut b = Hasher::new();
        b.write_value(&Value::FuncRef(None));
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn nan_payloads_hash_distinctly() {
        let mut a = Hasher::new();
        a.write_value(&Value::F32(f32::from_bits(0x7fc00001)));
        let mut b = Hasher::new();
        b.write_value(&Value::F32(f32::from_bits(0x7fc00002)));
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn value_equality_is_bitwise_for_floats() {
        assert_eq!(Value::F64(f64::NAN), Value::F64(f64::NAN));
        assert_ne!(Value::F32(0.0), Value::F32(-0.0));
    }
}
