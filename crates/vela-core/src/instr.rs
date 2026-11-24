//! Instruction representation produced by the binary decoder and text parser.

use crate::types::{BlockType, ValType};

/// Alignment and offset of a memory access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MemArg {
    /// Alignment expressed as a power of two (the encoded `align` value).
    pub align: u32,
    pub offset: u32,
}

impl MemArg {
    pub fn new(align: u32, offset: u32) -> Self {
        MemArg { align, offset }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoadKind {
    I32,
    I64,
    F32,
    F64,
    I32x8S,
    I32x8U,
    I32x16S,
    I32x16U,
    I64x8S,
    I64x8U,
    I64x16S,
    I64x16U,
    I64x32S,
    I64x32U,
}

impl LoadKind {
    pub fn result_type(self) -> ValType {
        match self {
            LoadKind::I32 | LoadKind::I32x8S | LoadKind::I32x8U | LoadKind::I32x16S
            | LoadKind::I32x16U => ValType::I32,
            LoadKind::I64 | LoadKind::I64x8S | LoadKind::I64x8U | LoadKind::I64x16S
            | LoadKind::I64x16U | LoadKind::I64x32S | LoadKind::I64x32U => ValType::I64,
            LoadKind::F32 => ValType::F32,
            LoadKind::F64 => ValType::F64,
        }
    }

    pub fn width(self) -> u32 {
        match self {
            LoadKind::I32 | LoadKind::F32 | LoadKind::I64x32S | LoadKind::I64x32U => 4,
            LoadKind::I64 | LoadKind::F64 => 8,
            LoadKind::I32x8S | LoadKind::I32x8U | LoadKind::I64x8S | LoadKind::I64x8U => 1,
            _ => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            LoadKind::I32 => "i32.load",
            LoadKind::I64 => "i64.load",
            LoadKind::F32 => "f32.load",
            LoadKind::F64 => "f64.load",
            LoadKind::I32x8S => "i32.load8_s",
            LoadKind::I32x8U => "i32.load8_u",
            LoadKind::I32x16S => "i32.load16_s",
            LoadKind::I32x16U => "i32.load16_u",
            LoadKind::I64x8S => "i64.load8_s",
            LoadKind::I64x8U => "i64.load8_u",
            LoadKind::I64x16S => "i64.load16_s",
            LoadKind::I64x16U => "i64.load16_u",
            LoadKind::I64x32S => "i64.load32_s",
            LoadKind::I64x32U => "i64.load32_u",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StoreKind {
    I32,
    I64,
    F32,
    F64,
    I32x8,
    I32x16,
    I64x8,
    I64x16,
    I64x32,
}

impl StoreKind {
    pub fn value_type(self) -> ValType {
        match self {
            StoreKind::I32 | StoreKind::I32x8 | StoreKind::I32x16 => ValType::I32,
            StoreKind::I64 | StoreKind::I64x8 | StoreKind::I64x16 | StoreKind::I64x32 => {
                ValType::I64
            }
            StoreKind::F32 => ValType::F32,
            StoreKind::F64 => ValType::F64,
        }
    }

    pub fn width(self) -> u32 {
        match self {
            StoreKind::I32 | StoreKind::F32 | StoreKind::I64x32 => 4,
            StoreKind::I64 | StoreKind::F64 => 8,
            StoreKind::I32x8 | StoreKind::I64x8 => 1,
            StoreKind::I32x16 | StoreKind::I64x16 => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            StoreKind::I32 => "i32.store",
            StoreKind::I64 => "i64.store",
            StoreKind::F32 => "f32.store",
            StoreKind::F64 => "f64.store",
            StoreKind::I32x8 => "i32.store8",
            StoreKind::I32x16 => "i32.store16",
            StoreKind::I64x8 => "i64.store8",
            StoreKind::I64x16 => "i64.store16",
            StoreKind::I64x32 => "i64.store32",
        }
    }
}

/// A single WebAssembly instruction.
///
/// `End` and `Else` are explicit so that decoded bodies map one-to-one onto
/// the binary format and can be disassembled without side tables.
#[derive(Clone, Debug, PartialEq)]
pub enum Instr {
    Unreachable,
    Nop,
    Block(BlockType),
    Loop(BlockType),
    If(BlockType),
    Else,
    End,
    Br(u32),
    BrIf(u32),
    BrTable(Box<BrTable>),
    Return,
    Call(u32),
    CallIndirect { type_idx: u32, table_idx: u32 },
    Drop,
    Select,
    SelectT(Vec<ValType>),
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),
    TableGet(u32),
    TableSet(u32),
    TableInit { elem: u32, table: u32 },
    ElemDrop(u32),
    TableCopy { dst: u32, src: u32 },
    TableGrow(u32),
    TableSize(u32),
    TableFill(u32),
    MemorySize(u32),
    MemoryGrow(u32),
    MemoryInit { data: u32, mem: u32 },
    DataDrop(u32),
    MemoryCopy { dst: u32, src: u32 },
    MemoryFill(u32),
    Load { kind: LoadKind, mem: u32, arg: MemArg },
    Store { kind: StoreKind, mem: u32, arg: MemArg },
    I32Const(i32),
    I64Const(i64),
    F32Const(u32),
    F64Const(u64),
    RefNull(ValType),
    RefIsNull,
    RefFunc(u32),
    // Numeric operations.
    I32Eqz,
    I32Eq,
    I32Ne,
    I32LtS,
    I32LtU,
    I32GtS,
    I32GtU,
    I32LeS,
    I32LeU,
    I32GeS,
    I32GeU,
    I64Eqz,
    I64Eq,
    I64Ne,
    I64LtS,
    I64LtU,
    I64GtS,
    I64GtU,
    I64LeS,
    I64LeU,
    I64GeS,
    I64GeU,
    F32Eq,
    F32Ne,
    F32Lt,
    F32Gt,
    F32Le,
    F32Ge,
    F64Eq,
    F64Ne,
    F64Lt,
    F64Gt,
    F64Le,
    F64Ge,
    I32Clz,
    I32Ctz,
    I32Popcnt,
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32DivU,
    I32RemS,
    I32RemU,
    I32And,
    I32Or,
    I32Xor,
    I32Shl,
    I32ShrS,
    I32ShrU,
    I32Rotl,
    I32Rotr,
    I64Clz,
    I64Ctz,
    I64Popcnt,
    I64Add,
    I64Sub,
    I64Mul,
    I64DivS,
    I64DivU,
    I64RemS,
    I64RemU,
    I64And,
    I64Or,
    I64Xor,
    I64Shl,
    I64ShrS,
    I64ShrU,
    I64Rotl,
    I64Rotr,
    F32Abs,
    F32Neg,
    F32Ceil,
    F32Floor,
    F32Trunc,
    F32Nearest,
    F32Sqrt,
    F32Add,
    F32Sub,
    F32Mul,
    F32Div,
    F32Min,
    F32Max,
    F32Copysign,
    F64Abs,
    F64Neg,
    F64Ceil,
    F64Floor,
    F64Trunc,
    F64Nearest,
    F64Sqrt,
    F64Add,
    F64Sub,
    F64Mul,
    F64Div,
    F64Min,
    F64Max,
    F64Copysign,
    I32WrapI64,
    I32TruncF32S,
    I32TruncF32U,
    I32TruncF64S,
    I32TruncF64U,
    I64ExtendI32S,
    I64ExtendI32U,
    I64TruncF32S,
    I64TruncF32U,
    I64TruncF64S,
    I64TruncF64U,
    F32ConvertI32S,
    F32ConvertI32U,
    F32ConvertI64S,
    F32ConvertI64U,
    F32DemoteF64,
    F64ConvertI32S,
    F64ConvertI32U,
    F64ConvertI64S,
    F64ConvertI64U,
    F64PromoteF32,
    I32ReinterpretF32,
    I64ReinterpretF64,
    F32ReinterpretI32,
    F64ReinterpretI64,
    I32Extend8S,
    I32Extend16S,
    I64Extend8S,
    I64Extend16S,
    I64Extend32S,
    I32TruncSatF32S,
    I32TruncSatF32U,
    I32TruncSatF64S,
    I32TruncSatF64U,
    I64TruncSatF32S,
    I64TruncSatF32U,
    I64TruncSatF64S,
    I64TruncSatF64U,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrTable {
    pub targets: Vec<u32>,
    pub default: u32,
}

impl Instr {
    /// Stable mnemonic used by traces, disassembly and the Studio.
    pub fn name(&self) -> &'static str {
        use Instr::*;
        match self {
            Unreachable => "unreachable",
            Nop => "nop",
            Block(_) => "block",
            Loop(_) => "loop",
            If(_) => "if",
            Else => "else",
            End => "end",
            Br(_) => "br",
            BrIf(_) => "br_if",
            BrTable(_) => "br_table",
            Return => "return",
            Call(_) => "call",
            CallIndirect { .. } => "call_indirect",
            Drop => "drop",
            Select => "select",
            SelectT(_) => "select_t",
            LocalGet(_) => "local.get",
            LocalSet(_) => "local.set",
            LocalTee(_) => "local.tee",
            GlobalGet(_) => "global.get",
            GlobalSet(_) => "global.set",
            TableGet(_) => "table.get",
            TableSet(_) => "table.set",
            TableInit { .. } => "table.init",
            ElemDrop(_) => "elem.drop",
            TableCopy { .. } => "table.copy",
            TableGrow(_) => "table.grow",
            TableSize(_) => "table.size",
            TableFill(_) => "table.fill",
            MemorySize(_) => "memory.size",
            MemoryGrow(_) => "memory.grow",
            MemoryInit { .. } => "memory.init",
            DataDrop(_) => "data.drop",
            MemoryCopy { .. } => "memory.copy",
            MemoryFill(_) => "memory.fill",
            Load { kind, .. } => kind.name(),
            Store { kind, .. } => kind.name(),
            I32Const(_) => "i32.const",
            I64Const(_) => "i64.const",
            F32Const(_) => "f32.const",
            F64Const(_) => "f64.const",
            RefNull(_) => "ref.null",
            RefIsNull => "ref.is_null",
            RefFunc(_) => "ref.func",
            I32Eqz => "i32.eqz",
            I32Eq => "i32.eq",
            I32Ne => "i32.ne",
            I32LtS => "i32.lt_s",
            I32LtU => "i32.lt_u",
            I32GtS => "i32.gt_s",
            I32GtU => "i32.gt_u",
            I32LeS => "i32.le_s",
            I32LeU => "i32.le_u",
            I32GeS => "i32.ge_s",
            I32GeU => "i32.ge_u",
            I64Eqz => "i64.eqz",
            I64Eq => "i64.eq",
            I64Ne => "i64.ne",
            I64LtS => "i64.lt_s",
            I64LtU => "i64.lt_u",
            I64GtS => "i64.gt_s",
            I64GtU => "i64.gt_u",
            I64LeS => "i64.le_s",
            I64LeU => "i64.le_u",
            I64GeS => "i64.ge_s",
            I64GeU => "i64.ge_u",
            F32Eq => "f32.eq",
            F32Ne => "f32.ne",
            F32Lt => "f32.lt",
            F32Gt => "f32.gt",
            F32Le => "f32.le",
            F32Ge => "f32.ge",
            F64Eq => "f64.eq",
            F64Ne => "f64.ne",
            F64Lt => "f64.lt",
            F64Gt => "f64.gt",
            F64Le => "f64.le",
            F64Ge => "f64.ge",
            I32Clz => "i32.clz",
            I32Ctz => "i32.ctz",
            I32Popcnt => "i32.popcnt",
            I32Add => "i32.add",
            I32Sub => "i32.sub",
            I32Mul => "i32.mul",
            I32DivS => "i32.div_s",
            I32DivU => "i32.div_u",
            I32RemS => "i32.rem_s",
            I32RemU => "i32.rem_u",
            I32And => "i32.and",
            I32Or => "i32.or",
            I32Xor => "i32.xor",
            I32Shl => "i32.shl",
            I32ShrS => "i32.shr_s",
            I32ShrU => "i32.shr_u",
            I32Rotl => "i32.rotl",
            I32Rotr => "i32.rotr",
            I64Clz => "i64.clz",
            I64Ctz => "i64.ctz",
            I64Popcnt => "i64.popcnt",
            I64Add => "i64.add",
            I64Sub => "i64.sub",
            I64Mul => "i64.mul",
            I64DivS => "i64.div_s",
            I64DivU => "i64.div_u",
            I64RemS => "i64.rem_s",
            I64RemU => "i64.rem_u",
            I64And => "i64.and",
            I64Or => "i64.or",
            I64Xor => "i64.xor",
            I64Shl => "i64.shl",
            I64ShrS => "i64.shr_s",
            I64ShrU => "i64.shr_u",
            I64Rotl => "i64.rotl",
            I64Rotr => "i64.rotr",
            F32Abs => "f32.abs",
            F32Neg => "f32.neg",
            F32Ceil => "f32.ceil",
            F32Floor => "f32.floor",
            F32Trunc => "f32.trunc",
            F32Nearest => "f32.nearest",
            F32Sqrt => "f32.sqrt",
            F32Add => "f32.add",
            F32Sub => "f32.sub",
            F32Mul => "f32.mul",
            F32Div => "f32.div",
            F32Min => "f32.min",
            F32Max => "f32.max",
            F32Copysign => "f32.copysign",
            F64Abs => "f64.abs",
            F64Neg => "f64.neg",
            F64Ceil => "f64.ceil",
            F64Floor => "f64.floor",
            F64Trunc => "f64.trunc",
            F64Nearest => "f64.nearest",
            F64Sqrt => "f64.sqrt",
            F64Add => "f64.add",
            F64Sub => "f64.sub",
            F64Mul => "f64.mul",
            F64Div => "f64.div",
            F64Min => "f64.min",
            F64Max => "f64.max",
            F64Copysign => "f64.copysign",
            I32WrapI64 => "i32.wrap_i64",
            I32TruncF32S => "i32.trunc_f32_s",
            I32TruncF32U => "i32.trunc_f32_u",
            I32TruncF64S => "i32.trunc_f64_s",
            I32TruncF64U => "i32.trunc_f64_u",
            I64ExtendI32S => "i64.extend_i32_s",
            I64ExtendI32U => "i64.extend_i32_u",
            I64TruncF32S => "i64.trunc_f32_s",
            I64TruncF32U => "i64.trunc_f32_u",
            I64TruncF64S => "i64.trunc_f64_s",
            I64TruncF64U => "i64.trunc_f64_u",
            F32ConvertI32S => "f32.convert_i32_s",
            F32ConvertI32U => "f32.convert_i32_u",
            F32ConvertI64S => "f32.convert_i64_s",
            F32ConvertI64U => "f32.convert_i64_u",
            F32DemoteF64 => "f32.demote_f64",
            F64ConvertI32S => "f64.convert_i32_s",
            F64ConvertI32U => "f64.convert_i32_u",
            F64ConvertI64S => "f64.convert_i64_s",
            F64ConvertI64U => "f64.convert_i64_u",
            F64PromoteF32 => "f64.promote_f32",
            I32ReinterpretF32 => "i32.reinterpret_f32",
            I64ReinterpretF64 => "i64.reinterpret_f64",
            F32ReinterpretI32 => "f32.reinterpret_i32",
            F64ReinterpretI64 => "f64.reinterpret_i64",
            I32Extend8S => "i32.extend8_s",
            I32Extend16S => "i32.extend16_s",
            I64Extend8S => "i64.extend8_s",
            I64Extend16S => "i64.extend16_s",
            I64Extend32S => "i64.extend32_s",
            I32TruncSatF32S => "i32.trunc_sat_f32_s",
            I32TruncSatF32U => "i32.trunc_sat_f32_u",
            I32TruncSatF64S => "i32.trunc_sat_f64_s",
            I32TruncSatF64U => "i32.trunc_sat_f64_u",
            I64TruncSatF32S => "i64.trunc_sat_f32_s",
            I64TruncSatF32U => "i64.trunc_sat_f32_u",
            I64TruncSatF64S => "i64.trunc_sat_f64_s",
            I64TruncSatF64U => "i64.trunc_sat_f64_u",
        }
    }

    /// True when the instruction ends a block (`end`/`else`).
    pub fn is_block_boundary(&self) -> bool {
        matches!(self, Instr::End | Instr::Else)
    }

    /// Numeric type signature for straight-line operators, used by the
    /// validator. Control and memory instructions are handled separately.
    pub fn numeric_signature(&self) -> Option<(&'static [ValType], &'static [ValType])> {
        use ValType::*;
        const I32_: &[ValType] = &[I32];
        const I64_: &[ValType] = &[I64];
        const F32_: &[ValType] = &[F32];
        const F64_: &[ValType] = &[F64];
        const I32_I32: &[ValType] = &[I32, I32];
        const I64_I64: &[ValType] = &[I64, I64];
        const F32_F32: &[ValType] = &[F32, F32];
        const F64_F64: &[ValType] = &[F64, F64];
        use Instr::*;
        Some(match self {
            I32Eqz | I32Clz | I32Ctz | I32Popcnt | I32Extend8S | I32Extend16S => (I32_, I32_),
            I64Eqz => (I64_, I32_),
            I64Clz | I64Ctz | I64Popcnt | I64Extend8S | I64Extend16S | I64Extend32S => {
                (I64_, I64_)
            }
            I32Eq | I32Ne | I32LtS | I32LtU | I32GtS | I32GtU | I32LeS | I32LeU | I32GeS
            | I32GeU | I32Add | I32Sub | I32Mul | I32DivS | I32DivU | I32RemS | I32RemU
            | I32And | I32Or | I32Xor | I32Shl | I32ShrS | I32ShrU | I32Rotl | I32Rotr => {
                (I32_I32, I32_)
            }
            I64Eq | I64Ne | I64LtS | I64LtU | I64GtS | I64GtU | I64LeS | I64LeU | I64GeS
            | I64GeU | I64Add | I64Sub | I64Mul | I64DivS | I64DivU | I64RemS | I64RemU
            | I64And | I64Or | I64Xor | I64Shl | I64ShrS | I64ShrU | I64Rotl | I64Rotr => {
                (I64_I64, I64_)
            }
            F32Eq | F32Ne | F32Lt | F32Gt | F32Le | F32Ge => (F32_F32, I32_),
            F64Eq | F64Ne | F64Lt | F64Gt | F64Le | F64Ge => (F64_F64, I32_),
            F32Abs | F32Neg | F32Ceil | F32Floor | F32Trunc | F32Nearest | F32Sqrt => (F32_, F32_),
            F64Abs | F64Neg | F64Ceil | F64Floor | F64Trunc | F64Nearest | F64Sqrt => (F64_, F64_),
            F32Add | F32Sub | F32Mul | F32Div | F32Min | F32Max | F32Copysign => (F32_F32, F32_),
            F64Add | F64Sub | F64Mul | F64Div | F64Min | F64Max | F64Copysign => (F64_F64, F64_),
            I32WrapI64 => (I64_, I32_),
            I32TruncF32S | I32TruncF32U | I32ReinterpretF32 => (F32_, I32_),
            I32TruncF64S | I32TruncF64U => (F64_, I32_),
            I64ExtendI32S | I64ExtendI32U => (I32_, I64_),
            I64TruncF32S | I64TruncF32U => (F32_, I64_),
            I64TruncF64S | I64TruncF64U | I64ReinterpretF64 => (F64_, I64_),
            F32ConvertI32S | F32ConvertI32U | F32ReinterpretI32 => (I32_, F32_),
            F32ConvertI64S | F32ConvertI64U => (I64_, F32_),
            F32DemoteF64 => (F64_, F32_),
            F64ConvertI32S | F64ConvertI32U => (I32_, F64_),
            F64ConvertI64S | F64ConvertI64U | F64ReinterpretI64 => (I64_, F64_),
            F64PromoteF32 => (F32_, F64_),
            I32TruncSatF32S | I32TruncSatF32U => (F32_, I32_),
            I32TruncSatF64S | I32TruncSatF64U => (F64_, I32_),
            I64TruncSatF32S | I64TruncSatF32U => (F32_, I64_),
            I64TruncSatF64S | I64TruncSatF64U => (F64_, I64_),
            _ => return None,
        })
    }
}

/// Render an instruction as a compact WAT-like line for disassembly.
pub fn disassemble(instr: &Instr) -> String {
    use Instr::*;
    match instr {
        Block(t) | Loop(t) | If(t) => match t {
            BlockType::Empty => instr.name().to_string(),
            BlockType::Value(v) => format!("{} (result {})", instr.name(), v),
            BlockType::TypeIdx(i) => format!("{} (type {})", instr.name(), i),
        },
        Br(l) => format!("br {l}"),
        BrIf(l) => format!("br_if {l}"),
        BrTable(t) => {
            let mut s = String::from("br_table");
            for l in &t.targets {
                s.push_str(&format!(" {l}"));
            }
            s.push_str(&format!(" {})", t.default));
            s
        }
        Call(f) => format!("call {f}"),
        CallIndirect { type_idx, table_idx } => format!("call_indirect (type {type_idx}) {table_idx}"),
        LocalGet(i) | LocalSet(i) | LocalTee(i) | GlobalGet(i) | GlobalSet(i) | TableGet(i)
        | TableSet(i) | ElemDrop(i) | TableGrow(i) | TableSize(i) | TableFill(i)
        | MemorySize(i) | MemoryGrow(i) | DataDrop(i) | MemoryFill(i) => {
            format!("{} {}", instr.name(), i)
        }
        TableInit { elem, table } => format!("table.init {table} {elem}"),
        TableCopy { dst, src } => format!("table.copy {dst} {src}"),
        MemoryInit { data, mem } => format!("memory.init {mem} {data}"),
        MemoryCopy { dst, src } => format!("memory.copy {dst} {src}"),
        Load { kind, mem, arg } => format!("{} {mem} offset={}", kind.name(), arg.offset),
        Store { kind, mem, arg } => format!("{} {mem} offset={}", kind.name(), arg.offset),
        I32Const(v) => format!("i32.const {v}"),
        I64Const(v) => format!("i64.const {v}"),
        F32Const(bits) => format!("f32.const {}", f32::from_bits(*bits)),
        F64Const(bits) => format!("f64.const {}", f64::from_bits(*bits)),
        RefNull(t) => format!("ref.null {}", t.name()),
        RefFunc(f) => format!("ref.func {f}"),
        SelectT(ts) => {
            let mut s = String::from("select");
            for t in ts {
                s.push_str(&format!(" (result {})", t.name()));
            }
            s
        }
        _ => instr.name().to_string(),
    }
}
