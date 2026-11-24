//! Mutable and immutable globals.

use crate::runtime::trap::Trap;
use crate::runtime::value::Value;
use crate::types::{GlobalType, ValType};

#[derive(Clone, Debug)]
pub struct GlobalInst {
    value: Value,
    ty: ValType,
    mutable: bool,
}

impl GlobalInst {
    pub fn new(ty: GlobalType, value: Value) -> Self {
        GlobalInst { value, ty: ty.val, mutable: ty.mutable }
    }

    pub fn get(&self) -> Value {
        self.value
    }

    pub fn set(&mut self, v: Value) -> Result<(), Trap> {
        if !self.mutable {
            return Err(Trap::TypeMismatch("immutable global write"));
        }
        if v.ty() != self.ty {
            return Err(Trap::TypeMismatch("global type"));
        }
        self.value = v;
        Ok(())
    }

    pub fn ty(&self) -> ValType {
        self.ty
    }

    pub fn is_mutable(&self) -> bool {
        self.mutable
    }
}
