//! Tables of references with bounded growth.

use crate::runtime::trap::Trap;
use crate::runtime::value::Value;
use crate::types::{Limits, ValType};

pub const DEFAULT_MAX_TABLE_ENTRIES: u32 = 1_000_000;

#[derive(Clone, Debug)]
pub struct TableInst {
    elems: Vec<Value>,
    elem_type: ValType,
    min: u32,
    max: Option<u32>,
    entry_cap: u32,
}

impl TableInst {
    pub fn new(elem_type: ValType, limits: Limits) -> Self {
        Self::try_new(elem_type, limits, DEFAULT_MAX_TABLE_ENTRIES)
            .unwrap_or_else(|| TableInst {
                elems: Vec::new(),
                elem_type,
                min: 0,
                max: Some(0),
                entry_cap: 0,
            })
    }

    pub fn try_new(elem_type: ValType, limits: Limits, entry_cap: u32) -> Option<Self> {
        if limits.min > entry_cap {
            return None;
        }
        let null = match elem_type {
            ValType::FuncRef => Value::FuncRef(None),
            _ => Value::ExternRef(None),
        };
        let mut elems = Vec::new();
        elems.try_reserve_exact(limits.min as usize).ok()?;
        elems.resize(limits.min as usize, null);
        Some(TableInst {
            elems,
            elem_type,
            min: limits.min,
            max: limits.max,
            entry_cap,
        })
    }

    pub fn elem_type(&self) -> ValType {
        self.elem_type
    }

    pub fn limits(&self) -> Limits {
        Limits::new(self.min, self.max)
    }

    pub fn len(&self) -> u32 {
        self.elems.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.elems.is_empty()
    }

    pub fn get(&self, i: u32) -> Result<Value, Trap> {
        self.elems.get(i as usize).copied().ok_or(Trap::TableOutOfBounds)
    }

    pub fn set(&mut self, i: u32, v: Value) -> Result<(), Trap> {
        if v.ty() != self.elem_type {
            return Err(Trap::TypeMismatch("table element type"));
        }
        let slot = self.elems.get_mut(i as usize).ok_or(Trap::TableOutOfBounds)?;
        *slot = v;
        Ok(())
    }

    pub fn grow(&mut self, delta: u32, init: Value) -> Option<u32> {
        let old = self.len();
        let new = old.checked_add(delta)?;
        if let Some(max) = self.max {
            if new > max {
                return None;
            }
        }
        if new > self.entry_cap {
            return None;
        }
        if self.elems.try_reserve_exact(delta as usize).is_err() {
            return None;
        }
        self.elems.resize(new as usize, init);
        Some(old)
    }

    pub fn fill(&mut self, dst: u32, val: Value, len: u32) -> Result<(), Trap> {
        if val.ty() != self.elem_type {
            return Err(Trap::TypeMismatch("table fill type"));
        }
        let d = dst as u64;
        let n = len as u64;
        if d + n > self.elems.len() as u64 {
            return Err(Trap::TableOutOfBounds);
        }
        let d = d as usize;
        for slot in &mut self.elems[d..d + n as usize] {
            *slot = val;
        }
        Ok(())
    }

    pub fn init(&mut self, dst: u32, src: &[Value], src_off: u32, len: u32) -> Result<(), Trap> {
        let s = src_off as u64;
        let d = dst as u64;
        let n = len as u64;
        if s + n > src.len() as u64 || d + n > self.elems.len() as u64 {
            return Err(Trap::TableOutOfBounds);
        }
        let (s, d, n) = (s as usize, d as usize, n as usize);
        for (i, v) in src[s..s + n].iter().enumerate() {
            if v.ty() != self.elem_type {
                return Err(Trap::TypeMismatch("table init type"));
            }
            self.elems[d + i] = *v;
        }
        Ok(())
    }

    pub fn copy(&mut self, dst: u32, src: u32, len: u32) -> Result<(), Trap> {
        let d = dst as u64;
        let s = src as u64;
        let n = len as u64;
        if s + n > self.elems.len() as u64 || d + n > self.elems.len() as u64 {
            return Err(Trap::TableOutOfBounds);
        }
        let (d, s, n) = (d as usize, s as usize, n as usize);
        self.elems.copy_within(s..s + n, d);
        Ok(())
    }

    pub fn elements(&self) -> &[Value] {
        &self.elems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_starts_null() {
        let t = TableInst::new(ValType::FuncRef, Limits::new(3, Some(4)));
        assert_eq!(t.len(), 3);
        assert_eq!(t.get(0), Ok(Value::FuncRef(None)));
        assert_eq!(t.get(3), Err(Trap::TableOutOfBounds));
    }

    #[test]
    fn table_grow_and_fill() {
        let mut t = TableInst::new(ValType::FuncRef, Limits::new(1, Some(3)));
        assert_eq!(t.grow(2, Value::FuncRef(Some(9))), Some(1));
        assert_eq!(t.get(2), Ok(Value::FuncRef(Some(9))));
        assert_eq!(t.grow(1, Value::FuncRef(None)), None);
        t.fill(0, Value::FuncRef(Some(1)), 3).unwrap();
        assert_eq!(t.get(0), Ok(Value::FuncRef(Some(1))));
    }

    #[test]
    fn rejects_wrong_reference_type() {
        let mut t = TableInst::new(ValType::FuncRef, Limits::new(1, None));
        assert_eq!(t.set(0, Value::ExternRef(None)), Err(Trap::TypeMismatch("table element type")));
    }
}
