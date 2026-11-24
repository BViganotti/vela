//! Section and instruction decoder for the WebAssembly binary format.

use crate::error::DecodeError;
use crate::instr::{BrTable, Instr, LoadKind, MemArg, StoreKind};
use crate::module::{
    Data, DataMode, Element, ElementMode, Export, ExportDesc, Func, FuncBody, Global, Import,
    ImportDesc, Module,
};
use crate::types::{
    BlockType, FuncType, GlobalType, Limits, MemoryType, TableType, ValType, MAX_PAGES,
};

use super::reader::{Reader, MAX_MODULE_BYTES};

const MAX_TYPES: usize = 100_000;
const MAX_IMPORTS: usize = 100_000;
const MAX_FUNCS: usize = 1_000_000;
const MAX_TABLES: usize = 100_000;
const MAX_MEMORIES: usize = 100;
const MAX_GLOBALS: usize = 100_000;
const MAX_EXPORTS: usize = 100_000;
const MAX_ELEMS: usize = 100_000;
const MAX_DATAS: usize = 100_000;
const MAX_LOCALS: usize = 50_000;
const MAX_PARAMS: usize = 10_000;
const MAX_RESULTS: usize = 10_000;
const MAX_NESTING: usize = 1_000;
const MAX_BODY_INSTRS: usize = 4_000_000;
const MAX_SEGMENT_ITEMS: usize = 1_000_000;

pub fn decode_module(bytes: &[u8]) -> Result<Module, DecodeError> {
    if bytes.len() > MAX_MODULE_BYTES {
        return Err(DecodeError::new(0, "module too large"));
    }
    let mut r = Reader::new(bytes);
    let magic = r.read_bytes(4)?;
    if magic != b"\0asm" {
        return Err(DecodeError::new(0, "magic header not detected"));
    }
    let version = r.read_bytes(4)?;
    if version != [1, 0, 0, 0] {
        return Err(DecodeError::new(4, "unknown binary version"));
    }

    let mut module = Module::default();
    let mut seen = [false; 13];
    let mut order = 0u8;
    let mut func_type_indices: Vec<u32> = Vec::new();
    let mut code_seen = false;

    while !r.is_empty() {
        let id = r.read_u8()?;
        let size = r.read_u32()? as usize;
        if size > r.remaining() {
            return Err(DecodeError::new(r.pos(), "section size out of bounds"));
        }
        let mut sr = r.sub_reader(size)?;
        if id > 12 {
            return Err(DecodeError::new(r.pos(), "malformed section id"));
        }
        if id != 0 {
            if seen[id as usize] {
                return Err(DecodeError::new(r.pos(), "duplicate section"));
            }
            seen[id as usize] = true;
            let ord = section_order(id);
            if ord < order {
                return Err(DecodeError::new(r.pos(), "section out of order"));
            }
            order = ord;
        }
        match id {
            0 => {
                let name = sr.read_name().unwrap_or_default();
                module.custom_sections.push(name);
                // Arbitrary payload: ignore remainder.
                continue;
            }
            1 => decode_type_section(&mut sr, &mut module)?,
            2 => decode_import_section(&mut sr, &mut module)?,
            3 => {
                let n = sr.read_vec_count("function", MAX_FUNCS)?;
                func_type_indices = Vec::with_capacity(n);
                for _ in 0..n {
                    func_type_indices.push(sr.read_u32()?);
                }
            }
            4 => decode_table_section(&mut sr, &mut module)?,
            5 => decode_memory_section(&mut sr, &mut module)?,
            6 => decode_global_section(&mut sr, &mut module)?,
            7 => decode_export_section(&mut sr, &mut module)?,
            8 => {
                module.start = Some(sr.read_u32()?);
            }
            9 => decode_element_section(&mut sr, &mut module)?,
            10 => {
                code_seen = true;
                let n = sr.read_vec_count("code", MAX_FUNCS)?;
                if n != func_type_indices.len() {
                    return Err(DecodeError::new(
                        sr.pos(),
                        "function and code section have inconsistent lengths",
                    ));
                }
                module.funcs.reserve(n);
                for type_idx in func_type_indices.drain(..) {
                    let body_size = sr.read_u32()? as usize;
                    if body_size > sr.remaining() {
                        return Err(DecodeError::new(sr.pos(), "function body too large"));
                    }
                    let mut br = sr.sub_reader(body_size)?;
                    let body = decode_func_body(&mut br)?;
                    if !br.is_empty() {
                        return Err(DecodeError::new(
                            br.pos(),
                            "trailing bytes in function body",
                        ));
                    }
                    module.funcs.push(Func { type_idx, body });
                }
            }
            11 => decode_data_section(&mut sr, &mut module)?,
            12 => {
                module.data_count = Some(sr.read_u32()?);
            }
            _ => unreachable!(),
        }
        if !sr.is_empty() {
            return Err(DecodeError::new(sr.pos(), "section size mismatch"));
        }
    }

    if !func_type_indices.is_empty() && !code_seen {
        return Err(DecodeError::new(bytes.len(), "missing code section"));
    }
    if !seen[11] && !module.datas.is_empty() {
        return Err(DecodeError::new(bytes.len(), "missing data section"));
    }
    Ok(module)
}

fn section_order(id: u8) -> u8 {
    // DataCount (12) is ordered between Element (9) and Code (10).
    match id {
        12 => 10,
        10 => 11,
        11 => 12,
        other => other,
    }
}

fn val_type(b: u8, r: &Reader<'_>) -> Result<ValType, DecodeError> {
    match b {
        0x7F => Ok(ValType::I32),
        0x7E => Ok(ValType::I64),
        0x7D => Ok(ValType::F32),
        0x7C => Ok(ValType::F64),
        0x70 => Ok(ValType::FuncRef),
        0x6F => Ok(ValType::ExternRef),
        _ => r.err("malformed value type"),
    }
}

fn read_val_type(r: &mut Reader<'_>) -> Result<ValType, DecodeError> {
    let b = r.read_u8()?;
    val_type(b, r)
}

fn read_limits(r: &mut Reader<'_>, max: u32) -> Result<Limits, DecodeError> {
    let flags = r.read_u32()?;
    let limits = match flags {
        0x00 => Limits::new(r.read_u32()?, None),
        0x01 => {
            let min = r.read_u32()?;
            let max = r.read_u32()?;
            Limits::new(min, Some(max))
        }
        _ => return r.err("malformed limits flags (shared or unknown memory)"),
    };
    if !limits.is_valid(max) {
        return r.err("limits out of range");
    }
    Ok(limits)
}

fn decode_type_section(r: &mut Reader<'_>, module: &mut Module) -> Result<(), DecodeError> {
    let n = r.read_vec_count("type", MAX_TYPES)?;
    for _ in 0..n {
        let form = r.read_u8()?;
        if form != 0x60 {
            return r.err("malformed function type");
        }
        let nparams = r.read_vec_count("parameter", MAX_PARAMS)?;
        let mut params = Vec::with_capacity(nparams);
        for _ in 0..nparams {
            params.push(read_val_type(r)?);
        }
        let nresults = r.read_vec_count("result", MAX_RESULTS)?;
        let mut results = Vec::with_capacity(nresults);
        for _ in 0..nresults {
            results.push(read_val_type(r)?);
        }
        module.types.push(FuncType { params, results });
    }
    Ok(())
}

fn decode_import_section(r: &mut Reader<'_>, module: &mut Module) -> Result<(), DecodeError> {
    let n = r.read_vec_count("import", MAX_IMPORTS)?;
    for _ in 0..n {
        let m = r.read_name()?;
        let name = r.read_name()?;
        let kind = r.read_u8()?;
        let desc = match kind {
            0x00 => ImportDesc::Func(r.read_u32()?),
            0x01 => {
                let elem = read_val_type(r)?;
                let limits = read_limits(r, MAX_PAGES)?;
                ImportDesc::Table(TableType { elem, limits })
            }
            0x02 => ImportDesc::Memory(MemoryType {
                limits: read_limits(r, MAX_PAGES)?,
            }),
            0x03 => {
                let val = read_val_type(r)?;
                let mutable = match r.read_u8()? {
                    0 => false,
                    1 => true,
                    _ => return r.err("malformed mutability"),
                };
                ImportDesc::Global(GlobalType { val, mutable })
            }
            _ => return r.err("malformed import kind"),
        };
        module.imports.push(Import {
            module: m,
            name,
            desc,
        });
    }
    Ok(())
}

fn decode_table_section(r: &mut Reader<'_>, module: &mut Module) -> Result<(), DecodeError> {
    let n = r.read_vec_count("table", MAX_TABLES)?;
    for _ in 0..n {
        let elem = read_val_type(r)?;
        if !elem.is_ref() {
            return r.err("table element type must be a reference type");
        }
        let limits = read_limits(r, crate::types::MAX_TABLE_ENTRIES)?;
        module.tables.push(TableType { elem, limits });
    }
    Ok(())
}

fn decode_memory_section(r: &mut Reader<'_>, module: &mut Module) -> Result<(), DecodeError> {
    let n = r.read_vec_count("memory", MAX_MEMORIES)?;
    if n > 1 {
        // Multi-memory is out of scope for Vela v1.
        return r.err("multiple memories are not supported");
    }
    for _ in 0..n {
        let limits = read_limits(r, MAX_PAGES)?;
        module.memories.push(MemoryType { limits });
    }
    Ok(())
}

fn decode_global_section(r: &mut Reader<'_>, module: &mut Module) -> Result<(), DecodeError> {
    let n = r.read_vec_count("global", MAX_GLOBALS)?;
    for _ in 0..n {
        let val = read_val_type(r)?;
        let mutable = match r.read_u8()? {
            0 => false,
            1 => true,
            _ => return r.err("malformed mutability"),
        };
        let init = read_expr(r)?;
        module.globals.push(Global {
            ty: GlobalType { val, mutable },
            init,
        });
    }
    Ok(())
}

fn decode_export_section(r: &mut Reader<'_>, module: &mut Module) -> Result<(), DecodeError> {
    let n = r.read_vec_count("export", MAX_EXPORTS)?;
    for _ in 0..n {
        let name = r.read_name()?;
        let kind = r.read_u8()?;
        let idx = r.read_u32()?;
        let desc = match kind {
            0x00 => ExportDesc::Func(idx),
            0x01 => ExportDesc::Table(idx),
            0x02 => ExportDesc::Memory(idx),
            0x03 => ExportDesc::Global(idx),
            _ => return r.err("malformed export kind"),
        };
        module.exports.push(Export { name, desc });
    }
    Ok(())
}

fn decode_element_section(r: &mut Reader<'_>, module: &mut Module) -> Result<(), DecodeError> {
    let n = r.read_vec_count("element", MAX_ELEMS)?;
    for _ in 0..n {
        let flags = r.read_u32()?;
        match flags {
            0 | 2 | 4 | 6 => {
                let table = if flags == 0 || flags == 4 {
                    0
                } else {
                    r.read_u32()?
                };
                let offset = read_expr(r)?;
                let (elem_type, init) = read_elem_items(r, flags >= 4)?;
                module.elems.push(Element {
                    mode: ElementMode::Active { table, offset },
                    elem_type,
                    init,
                });
            }
            1 | 3 => {
                let kind = r.read_u8()?;
                if kind != 0x00 {
                    return r.err("malformed element kind");
                }
                let init = read_func_indices(r)?;
                module.elems.push(Element {
                    mode: if flags == 1 {
                        ElementMode::Passive
                    } else {
                        ElementMode::Declarative
                    },
                    elem_type: ValType::FuncRef,
                    init,
                });
            }
            5 | 7 => {
                let ty = read_val_type(r)?;
                if !ty.is_ref() {
                    return r.err("element type must be a reference type");
                }
                let init = read_elem_exprs(r)?;
                module.elems.push(Element {
                    mode: if flags == 5 {
                        ElementMode::Passive
                    } else {
                        ElementMode::Declarative
                    },
                    elem_type: ty,
                    init,
                });
            }
            _ => return r.err("malformed element segment flags"),
        }
    }
    Ok(())
}

fn read_func_indices(r: &mut Reader<'_>) -> Result<Vec<Vec<Instr>>, DecodeError> {
    let n = r.read_vec_count("element item", MAX_SEGMENT_ITEMS)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let idx = r.read_u32()?;
        out.push(vec![Instr::RefFunc(idx), Instr::End]);
    }
    Ok(out)
}

fn read_elem_exprs(r: &mut Reader<'_>) -> Result<Vec<Vec<Instr>>, DecodeError> {
    let n = r.read_vec_count("element item", MAX_SEGMENT_ITEMS)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(read_expr(r)?);
    }
    Ok(out)
}

fn read_elem_items(
    r: &mut Reader<'_>,
    exprs: bool,
) -> Result<(ValType, Vec<Vec<Instr>>), DecodeError> {
    if exprs {
        // Active segment with expressions carries an explicit reference type.
        let ty = read_val_type(r)?;
        if !ty.is_ref() {
            return r.err("element type must be a reference type");
        }
        Ok((ty, read_elem_exprs(r)?))
    } else {
        let kind = r.read_u8()?;
        if kind != 0x00 {
            return r.err("malformed element kind");
        }
        Ok((ValType::FuncRef, read_func_indices(r)?))
    }
}

fn decode_data_section(r: &mut Reader<'_>, module: &mut Module) -> Result<(), DecodeError> {
    let n = r.read_vec_count("data", MAX_DATAS)?;
    for _ in 0..n {
        let flags = r.read_u32()?;
        match flags {
            0 => {
                let offset = read_expr(r)?;
                let bytes = read_byte_vec(r)?;
                module.datas.push(Data {
                    mode: DataMode::Active { memory: 0, offset },
                    bytes,
                });
            }
            1 => {
                let bytes = read_byte_vec(r)?;
                module.datas.push(Data {
                    mode: DataMode::Passive,
                    bytes,
                });
            }
            2 => {
                let memory = r.read_u32()?;
                let offset = read_expr(r)?;
                let bytes = read_byte_vec(r)?;
                module.datas.push(Data {
                    mode: DataMode::Active { memory, offset },
                    bytes,
                });
            }
            _ => return r.err("malformed data segment flags"),
        }
    }
    Ok(())
}

fn read_byte_vec(r: &mut Reader<'_>) -> Result<Vec<u8>, DecodeError> {
    let n = r.read_u32()? as usize;
    if n > r.remaining() {
        return r.err("data segment out of bounds");
    }
    Ok(r.read_bytes(n)?.to_vec())
}

fn decode_func_body(r: &mut Reader<'_>) -> Result<FuncBody, DecodeError> {
    let groups = r.read_vec_count("local group", MAX_LOCALS)?;
    let mut locals = Vec::new();
    for _ in 0..groups {
        let count = r.read_u32()? as usize;
        if count > MAX_LOCALS || locals.len() + count > MAX_LOCALS {
            return r.err("too many locals");
        }
        let ty = read_val_type(r)?;
        locals.extend(std::iter::repeat(ty).take(count));
    }
    let body = read_expr(r)?;
    Ok(FuncBody { locals, body })
}

/// Read an instruction expression up to and including its matching `end`.
fn read_expr(r: &mut Reader<'_>) -> Result<Vec<Instr>, DecodeError> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    loop {
        if out.len() > MAX_BODY_INSTRS {
            return r.err("expression too large");
        }
        let instr = decode_instr(r)?;
        match &instr {
            Instr::Block(_) | Instr::Loop(_) | Instr::If(_) => {
                depth += 1;
                if depth > MAX_NESTING {
                    return r.err("expression nesting too deep");
                }
            }
            Instr::End => {
                out.push(instr);
                if depth == 0 {
                    return Ok(out);
                }
                depth -= 1;
                continue;
            }
            Instr::Else if depth == 0 => {
                return r.err("else outside of if");
            }
            _ => {}
        }
        out.push(instr);
    }
}

fn read_block_type(r: &mut Reader<'_>) -> Result<BlockType, DecodeError> {
    match r.peek_u8() {
        Some(0x40) => {
            r.read_u8()?;
            Ok(BlockType::Empty)
        }
        Some(b) if matches!(b, 0x7F | 0x7E | 0x7D | 0x7C | 0x70 | 0x6F) => {
            Ok(BlockType::Value(read_val_type(r)?))
        }
        _ => {
            let idx = r.read_s33()?;
            if idx < 0 || idx > u32::MAX as i64 {
                return r.err("malformed block type");
            }
            Ok(BlockType::TypeIdx(idx as u32))
        }
    }
}

fn read_memarg(r: &mut Reader<'_>) -> Result<MemArg, DecodeError> {
    let align = r.read_u32()?;
    let offset = r.read_u32()?;
    Ok(MemArg { align, offset })
}

/// Decode exactly one instruction (control instructions are not nested here).
pub fn decode_instr(r: &mut Reader<'_>) -> Result<Instr, DecodeError> {
    let op = r.read_u8()?;
    let instr = match op {
        0x00 => Instr::Unreachable,
        0x01 => Instr::Nop,
        0x02 => Instr::Block(read_block_type(r)?),
        0x03 => Instr::Loop(read_block_type(r)?),
        0x04 => Instr::If(read_block_type(r)?),
        0x05 => Instr::Else,
        0x0B => Instr::End,
        0x0C => Instr::Br(r.read_u32()?),
        0x0D => Instr::BrIf(r.read_u32()?),
        0x0E => {
            let n = r.read_vec_count("br_table", MAX_BODY_INSTRS)?;
            let mut targets = Vec::with_capacity(n);
            for _ in 0..n {
                targets.push(r.read_u32()?);
            }
            let default = r.read_u32()?;
            Instr::BrTable(Box::new(BrTable { targets, default }))
        }
        0x0F => Instr::Return,
        0x10 => Instr::Call(r.read_u32()?),
        0x11 => {
            let type_idx = r.read_u32()?;
            let table_idx = r.read_u32()?;
            Instr::CallIndirect {
                type_idx,
                table_idx,
            }
        }
        0x1A => Instr::Drop,
        0x1B => Instr::Select,
        0x1C => {
            let n = r.read_vec_count("select result", MAX_RESULTS)?;
            let mut tys = Vec::with_capacity(n);
            for _ in 0..n {
                tys.push(read_val_type(r)?);
            }
            Instr::SelectT(tys)
        }
        0x20 => Instr::LocalGet(r.read_u32()?),
        0x21 => Instr::LocalSet(r.read_u32()?),
        0x22 => Instr::LocalTee(r.read_u32()?),
        0x23 => Instr::GlobalGet(r.read_u32()?),
        0x24 => Instr::GlobalSet(r.read_u32()?),
        0x25 => Instr::TableGet(r.read_u32()?),
        0x26 => Instr::TableSet(r.read_u32()?),
        0x28 => load(r, LoadKind::I32)?,
        0x29 => load(r, LoadKind::I64)?,
        0x2A => load(r, LoadKind::F32)?,
        0x2B => load(r, LoadKind::F64)?,
        0x2C => load(r, LoadKind::I32x8S)?,
        0x2D => load(r, LoadKind::I32x8U)?,
        0x2E => load(r, LoadKind::I32x16S)?,
        0x2F => load(r, LoadKind::I32x16U)?,
        0x30 => load(r, LoadKind::I64x8S)?,
        0x31 => load(r, LoadKind::I64x8U)?,
        0x32 => load(r, LoadKind::I64x16S)?,
        0x33 => load(r, LoadKind::I64x16U)?,
        0x34 => load(r, LoadKind::I64x32S)?,
        0x35 => load(r, LoadKind::I64x32U)?,
        0x36 => store(r, StoreKind::I32)?,
        0x37 => store(r, StoreKind::I64)?,
        0x38 => store(r, StoreKind::F32)?,
        0x39 => store(r, StoreKind::F64)?,
        0x3A => store(r, StoreKind::I32x8)?,
        0x3B => store(r, StoreKind::I32x16)?,
        0x3C => store(r, StoreKind::I64x8)?,
        0x3D => store(r, StoreKind::I64x16)?,
        0x3E => store(r, StoreKind::I64x32)?,
        0x3F => Instr::MemorySize(r.read_u32()?),
        0x40 => Instr::MemoryGrow(r.read_u32()?),
        0x41 => Instr::I32Const(r.read_i32()?),
        0x42 => Instr::I64Const(r.read_i64()?),
        0x43 => Instr::F32Const(r.read_f32_bits()?),
        0x44 => Instr::F64Const(r.read_f64_bits()?),
        0x45 => Instr::I32Eqz,
        0x46 => Instr::I32Eq,
        0x47 => Instr::I32Ne,
        0x48 => Instr::I32LtS,
        0x49 => Instr::I32LtU,
        0x4A => Instr::I32GtS,
        0x4B => Instr::I32GtU,
        0x4C => Instr::I32LeS,
        0x4D => Instr::I32LeU,
        0x4E => Instr::I32GeS,
        0x4F => Instr::I32GeU,
        0x50 => Instr::I64Eqz,
        0x51 => Instr::I64Eq,
        0x52 => Instr::I64Ne,
        0x53 => Instr::I64LtS,
        0x54 => Instr::I64LtU,
        0x55 => Instr::I64GtS,
        0x56 => Instr::I64GtU,
        0x57 => Instr::I64LeS,
        0x58 => Instr::I64LeU,
        0x59 => Instr::I64GeS,
        0x5A => Instr::I64GeU,
        0x5B => Instr::F32Eq,
        0x5C => Instr::F32Ne,
        0x5D => Instr::F32Lt,
        0x5E => Instr::F32Gt,
        0x5F => Instr::F32Le,
        0x60 => Instr::F32Ge,
        0x61 => Instr::F64Eq,
        0x62 => Instr::F64Ne,
        0x63 => Instr::F64Lt,
        0x64 => Instr::F64Gt,
        0x65 => Instr::F64Le,
        0x66 => Instr::F64Ge,
        0x67 => Instr::I32Clz,
        0x68 => Instr::I32Ctz,
        0x69 => Instr::I32Popcnt,
        0x6A => Instr::I32Add,
        0x6B => Instr::I32Sub,
        0x6C => Instr::I32Mul,
        0x6D => Instr::I32DivS,
        0x6E => Instr::I32DivU,
        0x6F => Instr::I32RemS,
        0x70 => Instr::I32RemU,
        0x71 => Instr::I32And,
        0x72 => Instr::I32Or,
        0x73 => Instr::I32Xor,
        0x74 => Instr::I32Shl,
        0x75 => Instr::I32ShrS,
        0x76 => Instr::I32ShrU,
        0x77 => Instr::I32Rotl,
        0x78 => Instr::I32Rotr,
        0x79 => Instr::I64Clz,
        0x7A => Instr::I64Ctz,
        0x7B => Instr::I64Popcnt,
        0x7C => Instr::I64Add,
        0x7D => Instr::I64Sub,
        0x7E => Instr::I64Mul,
        0x7F => Instr::I64DivS,
        0x80 => Instr::I64DivU,
        0x81 => Instr::I64RemS,
        0x82 => Instr::I64RemU,
        0x83 => Instr::I64And,
        0x84 => Instr::I64Or,
        0x85 => Instr::I64Xor,
        0x86 => Instr::I64Shl,
        0x87 => Instr::I64ShrS,
        0x88 => Instr::I64ShrU,
        0x89 => Instr::I64Rotl,
        0x8A => Instr::I64Rotr,
        0x8B => Instr::F32Abs,
        0x8C => Instr::F32Neg,
        0x8D => Instr::F32Ceil,
        0x8E => Instr::F32Floor,
        0x8F => Instr::F32Trunc,
        0x90 => Instr::F32Nearest,
        0x91 => Instr::F32Sqrt,
        0x92 => Instr::F32Add,
        0x93 => Instr::F32Sub,
        0x94 => Instr::F32Mul,
        0x95 => Instr::F32Div,
        0x96 => Instr::F32Min,
        0x97 => Instr::F32Max,
        0x98 => Instr::F32Copysign,
        0x99 => Instr::F64Abs,
        0x9A => Instr::F64Neg,
        0x9B => Instr::F64Ceil,
        0x9C => Instr::F64Floor,
        0x9D => Instr::F64Trunc,
        0x9E => Instr::F64Nearest,
        0x9F => Instr::F64Sqrt,
        0xA0 => Instr::F64Add,
        0xA1 => Instr::F64Sub,
        0xA2 => Instr::F64Mul,
        0xA3 => Instr::F64Div,
        0xA4 => Instr::F64Min,
        0xA5 => Instr::F64Max,
        0xA6 => Instr::F64Copysign,
        0xA7 => Instr::I32WrapI64,
        0xA8 => Instr::I32TruncF32S,
        0xA9 => Instr::I32TruncF32U,
        0xAA => Instr::I32TruncF64S,
        0xAB => Instr::I32TruncF64U,
        0xAC => Instr::I64ExtendI32S,
        0xAD => Instr::I64ExtendI32U,
        0xAE => Instr::I64TruncF32S,
        0xAF => Instr::I64TruncF32U,
        0xB0 => Instr::I64TruncF64S,
        0xB1 => Instr::I64TruncF64U,
        0xB2 => Instr::F32ConvertI32S,
        0xB3 => Instr::F32ConvertI32U,
        0xB4 => Instr::F32ConvertI64S,
        0xB5 => Instr::F32ConvertI64U,
        0xB6 => Instr::F32DemoteF64,
        0xB7 => Instr::F64ConvertI32S,
        0xB8 => Instr::F64ConvertI32U,
        0xB9 => Instr::F64ConvertI64S,
        0xBA => Instr::F64ConvertI64U,
        0xBB => Instr::F64PromoteF32,
        0xBC => Instr::I32ReinterpretF32,
        0xBD => Instr::I64ReinterpretF64,
        0xBE => Instr::F32ReinterpretI32,
        0xBF => Instr::F64ReinterpretI64,
        0xC0 => Instr::I32Extend8S,
        0xC1 => Instr::I32Extend16S,
        0xC2 => Instr::I64Extend8S,
        0xC3 => Instr::I64Extend16S,
        0xC4 => Instr::I64Extend32S,
        0xD0 => {
            let ty = read_val_type(r)?;
            if !ty.is_ref() {
                return r.err("ref.null requires a reference type");
            }
            Instr::RefNull(ty)
        }
        0xD1 => Instr::RefIsNull,
        0xD2 => Instr::RefFunc(r.read_u32()?),
        0xFC => decode_fc(r)?,
        _ => return r.err("illegal opcode"),
    };
    Ok(instr)
}

fn load(r: &mut Reader<'_>, kind: LoadKind) -> Result<Instr, DecodeError> {
    Ok(Instr::Load {
        kind,
        mem: 0,
        arg: read_memarg(r)?,
    })
}

fn store(r: &mut Reader<'_>, kind: StoreKind) -> Result<Instr, DecodeError> {
    Ok(Instr::Store {
        kind,
        mem: 0,
        arg: read_memarg(r)?,
    })
}

fn decode_fc(r: &mut Reader<'_>) -> Result<Instr, DecodeError> {
    let sub = r.read_u32()?;
    Ok(match sub {
        0 => Instr::I32TruncSatF32S,
        1 => Instr::I32TruncSatF32U,
        2 => Instr::I32TruncSatF64S,
        3 => Instr::I32TruncSatF64U,
        4 => Instr::I64TruncSatF32S,
        5 => Instr::I64TruncSatF32U,
        6 => Instr::I64TruncSatF64S,
        7 => Instr::I64TruncSatF64U,
        8 => {
            let data = r.read_u32()?;
            let mem = r.read_u32()?;
            Instr::MemoryInit { data, mem }
        }
        9 => Instr::DataDrop(r.read_u32()?),
        10 => {
            let dst = r.read_u32()?;
            let src = r.read_u32()?;
            Instr::MemoryCopy { dst, src }
        }
        11 => Instr::MemoryFill(r.read_u32()?),
        12 => {
            let elem = r.read_u32()?;
            let table = r.read_u32()?;
            Instr::TableInit { elem, table }
        }
        13 => Instr::ElemDrop(r.read_u32()?),
        14 => {
            let dst = r.read_u32()?;
            let src = r.read_u32()?;
            Instr::TableCopy { dst, src }
        }
        15 => Instr::TableGrow(r.read_u32()?),
        16 => Instr::TableSize(r.read_u32()?),
        17 => Instr::TableFill(r.read_u32()?),
        _ => return r.err("illegal opcode"),
    })
}

/// Encode a module back to bytes. Used by tests and the Studio's binary view.
pub fn encode_module(module: &Module) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\0asm");
    out.extend_from_slice(&[1, 0, 0, 0]);

    // Type section.
    let mut sec = Vec::new();
    put_u32(&mut sec, module.types.len() as u32);
    for ty in &module.types {
        sec.push(0x60);
        put_u32(&mut sec, ty.params.len() as u32);
        for p in &ty.params {
            sec.push(p.code());
        }
        put_u32(&mut sec, ty.results.len() as u32);
        for r in &ty.results {
            sec.push(r.code());
        }
    }
    put_section(&mut out, 1, &sec);

    // Import section.
    let mut sec = Vec::new();
    put_u32(&mut sec, module.imports.len() as u32);
    for imp in &module.imports {
        put_name(&mut sec, &imp.module);
        put_name(&mut sec, &imp.name);
        match imp.desc {
            ImportDesc::Func(t) => {
                sec.push(0);
                put_u32(&mut sec, t);
            }
            ImportDesc::Table(t) => {
                sec.push(1);
                sec.push(t.elem.code());
                put_limits(&mut sec, t.limits);
            }
            ImportDesc::Memory(m) => {
                sec.push(2);
                put_limits(&mut sec, m.limits);
            }
            ImportDesc::Global(g) => {
                sec.push(3);
                sec.push(g.val.code());
                sec.push(if g.mutable { 1 } else { 0 });
            }
        }
    }
    put_section(&mut out, 2, &sec);

    // Function section.
    let mut sec = Vec::new();
    put_u32(&mut sec, module.funcs.len() as u32);
    for f in &module.funcs {
        put_u32(&mut sec, f.type_idx);
    }
    put_section(&mut out, 3, &sec);

    let mut sec = Vec::new();
    put_u32(&mut sec, module.tables.len() as u32);
    for t in &module.tables {
        sec.push(t.elem.code());
        put_limits(&mut sec, t.limits);
    }
    put_section(&mut out, 4, &sec);

    let mut sec = Vec::new();
    put_u32(&mut sec, module.memories.len() as u32);
    for m in &module.memories {
        put_limits(&mut sec, m.limits);
    }
    put_section(&mut out, 5, &sec);

    let mut sec = Vec::new();
    put_u32(&mut sec, module.globals.len() as u32);
    for g in &module.globals {
        sec.push(g.ty.val.code());
        sec.push(if g.ty.mutable { 1 } else { 0 });
        put_expr(&mut sec, &g.init);
    }
    put_section(&mut out, 6, &sec);

    let mut sec = Vec::new();
    put_u32(&mut sec, module.exports.len() as u32);
    for e in &module.exports {
        put_name(&mut sec, &e.name);
        match e.desc {
            ExportDesc::Func(i) => {
                sec.push(0);
                put_u32(&mut sec, i);
            }
            ExportDesc::Table(i) => {
                sec.push(1);
                put_u32(&mut sec, i);
            }
            ExportDesc::Memory(i) => {
                sec.push(2);
                put_u32(&mut sec, i);
            }
            ExportDesc::Global(i) => {
                sec.push(3);
                put_u32(&mut sec, i);
            }
        }
    }
    put_section(&mut out, 7, &sec);

    if let Some(start) = module.start {
        let mut sec = Vec::new();
        put_u32(&mut sec, start);
        put_section(&mut out, 8, &sec);
    }

    let mut sec = Vec::new();
    put_u32(&mut sec, module.elems.len() as u32);
    for e in &module.elems {
        match &e.mode {
            ElementMode::Passive => {
                sec.push(5);
                sec.push(e.elem_type.code());
                put_expr_list(&mut sec, &e.init);
            }
            ElementMode::Declarative => {
                sec.push(7);
                sec.push(e.elem_type.code());
                put_expr_list(&mut sec, &e.init);
            }
            ElementMode::Active { table, offset } => {
                if *table == 0 {
                    sec.push(4);
                    put_expr(&mut sec, offset);
                    sec.push(e.elem_type.code());
                    put_expr_list(&mut sec, &e.init);
                } else {
                    sec.push(6);
                    put_u32(&mut sec, *table);
                    put_expr(&mut sec, offset);
                    sec.push(e.elem_type.code());
                    put_expr_list(&mut sec, &e.init);
                }
            }
        }
    }
    put_section(&mut out, 9, &sec);

    if let Some(dc) = module.data_count {
        let mut sec = Vec::new();
        put_u32(&mut sec, dc);
        put_section(&mut out, 12, &sec);
    }

    let mut sec = Vec::new();
    put_u32(&mut sec, module.funcs.len() as u32);
    for f in &module.funcs {
        let mut body = Vec::new();
        // Group consecutive locals of the same type.
        let mut groups: Vec<(u32, ValType)> = Vec::new();
        for l in &f.body.locals {
            match groups.last_mut() {
                Some((n, t)) if *t == *l => *n += 1,
                _ => groups.push((1, *l)),
            }
        }
        put_u32(&mut body, groups.len() as u32);
        for (n, t) in groups {
            put_u32(&mut body, n);
            body.push(t.code());
        }
        put_expr(&mut body, &f.body.body);
        put_u32(&mut sec, body.len() as u32);
        sec.extend_from_slice(&body);
    }
    put_section(&mut out, 10, &sec);

    let mut sec = Vec::new();
    put_u32(&mut sec, module.datas.len() as u32);
    for d in &module.datas {
        match &d.mode {
            DataMode::Passive => {
                sec.push(1);
            }
            DataMode::Active { memory: 0, offset } => {
                sec.push(0);
                put_expr(&mut sec, offset);
            }
            DataMode::Active { memory, offset } => {
                sec.push(2);
                put_u32(&mut sec, *memory);
                put_expr(&mut sec, offset);
            }
        }
        put_u32(&mut sec, d.bytes.len() as u32);
        sec.extend_from_slice(&d.bytes);
    }
    put_section(&mut out, 11, &sec);
    out
}

fn put_section(out: &mut Vec<u8>, id: u8, body: &[u8]) {
    out.push(id);
    put_u32(out, body.len() as u32);
    out.extend_from_slice(body);
}

fn put_u32(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

fn put_i32(out: &mut Vec<u8>, v: i32) {
    let mut v = v;
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        let sign = b & 0x40 != 0;
        if (v == 0 && !sign) || (v == -1 && sign) {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

fn put_i64(out: &mut Vec<u8>, v: i64) {
    let mut v = v;
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        let sign = b & 0x40 != 0;
        if (v == 0 && !sign) || (v == -1 && sign) {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

fn put_name(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn put_limits(out: &mut Vec<u8>, l: Limits) {
    match l.max {
        None => {
            out.push(0);
            put_u32(out, l.min);
        }
        Some(max) => {
            out.push(1);
            put_u32(out, l.min);
            put_u32(out, max);
        }
    }
}

fn put_expr(out: &mut Vec<u8>, body: &[Instr]) {
    for instr in body {
        put_instr(out, instr);
    }
}

fn put_expr_list(out: &mut Vec<u8>, items: &[Vec<Instr>]) {
    put_u32(out, items.len() as u32);
    for item in items {
        put_expr(out, item);
    }
}

fn put_block_type(out: &mut Vec<u8>, bt: BlockType) {
    match bt {
        BlockType::Empty => out.push(0x40),
        BlockType::Value(v) => out.push(v.code()),
        BlockType::TypeIdx(i) => put_i32(out, i as i32),
    }
}

/// Encode one instruction. Kept beside the decoder so round-trips stay honest.
pub fn put_instr(out: &mut Vec<u8>, instr: &Instr) {
    use Instr::*;
    match instr {
        Unreachable => out.push(0x00),
        Nop => out.push(0x01),
        Block(t) => {
            out.push(0x02);
            put_block_type(out, *t);
        }
        Loop(t) => {
            out.push(0x03);
            put_block_type(out, *t);
        }
        If(t) => {
            out.push(0x04);
            put_block_type(out, *t);
        }
        Else => out.push(0x05),
        End => out.push(0x0B),
        Br(l) => {
            out.push(0x0C);
            put_u32(out, *l);
        }
        BrIf(l) => {
            out.push(0x0D);
            put_u32(out, *l);
        }
        BrTable(t) => {
            out.push(0x0E);
            put_u32(out, t.targets.len() as u32);
            for x in &t.targets {
                put_u32(out, *x);
            }
            put_u32(out, t.default);
        }
        Return => out.push(0x0F),
        Call(f) => {
            out.push(0x10);
            put_u32(out, *f);
        }
        CallIndirect {
            type_idx,
            table_idx,
        } => {
            out.push(0x11);
            put_u32(out, *type_idx);
            put_u32(out, *table_idx);
        }
        Drop => out.push(0x1A),
        Select => out.push(0x1B),
        SelectT(tys) => {
            out.push(0x1C);
            put_u32(out, tys.len() as u32);
            for t in tys {
                out.push(t.code());
            }
        }
        LocalGet(i) => {
            out.push(0x20);
            put_u32(out, *i);
        }
        LocalSet(i) => {
            out.push(0x21);
            put_u32(out, *i);
        }
        LocalTee(i) => {
            out.push(0x22);
            put_u32(out, *i);
        }
        GlobalGet(i) => {
            out.push(0x23);
            put_u32(out, *i);
        }
        GlobalSet(i) => {
            out.push(0x24);
            put_u32(out, *i);
        }
        TableGet(i) => {
            out.push(0x25);
            put_u32(out, *i);
        }
        TableSet(i) => {
            out.push(0x26);
            put_u32(out, *i);
        }
        Load { kind, arg, .. } => {
            out.push(load_opcode(*kind));
            put_u32(out, arg.align);
            put_u32(out, arg.offset);
        }
        Store { kind, arg, .. } => {
            out.push(store_opcode(*kind));
            put_u32(out, arg.align);
            put_u32(out, arg.offset);
        }
        MemorySize(m) => {
            out.push(0x3F);
            put_u32(out, *m);
        }
        MemoryGrow(m) => {
            out.push(0x40);
            put_u32(out, *m);
        }
        I32Const(v) => {
            out.push(0x41);
            put_i32(out, *v);
        }
        I64Const(v) => {
            out.push(0x42);
            put_i64(out, *v);
        }
        F32Const(bits) => {
            out.push(0x43);
            out.extend_from_slice(&bits.to_le_bytes());
        }
        F64Const(bits) => {
            out.push(0x44);
            out.extend_from_slice(&bits.to_le_bytes());
        }
        RefNull(t) => {
            out.push(0xD0);
            out.push(t.code());
        }
        RefIsNull => out.push(0xD1),
        RefFunc(f) => {
            out.push(0xD2);
            put_u32(out, *f);
        }
        TableInit { elem, table } => {
            out.push(0xFC);
            put_u32(out, 12);
            put_u32(out, *elem);
            put_u32(out, *table);
        }
        ElemDrop(e) => {
            out.push(0xFC);
            put_u32(out, 13);
            put_u32(out, *e);
        }
        TableCopy { dst, src } => {
            out.push(0xFC);
            put_u32(out, 14);
            put_u32(out, *dst);
            put_u32(out, *src);
        }
        TableGrow(t) => {
            out.push(0xFC);
            put_u32(out, 15);
            put_u32(out, *t);
        }
        TableSize(t) => {
            out.push(0xFC);
            put_u32(out, 16);
            put_u32(out, *t);
        }
        TableFill(t) => {
            out.push(0xFC);
            put_u32(out, 17);
            put_u32(out, *t);
        }
        MemoryInit { data, mem } => {
            out.push(0xFC);
            put_u32(out, 8);
            put_u32(out, *data);
            put_u32(out, *mem);
        }
        DataDrop(d) => {
            out.push(0xFC);
            put_u32(out, 9);
            put_u32(out, *d);
        }
        MemoryCopy { dst, src } => {
            out.push(0xFC);
            put_u32(out, 10);
            put_u32(out, *dst);
            put_u32(out, *src);
        }
        MemoryFill(m) => {
            out.push(0xFC);
            put_u32(out, 11);
            put_u32(out, *m);
        }
        I32TruncSatF32S | I32TruncSatF32U | I32TruncSatF64S | I32TruncSatF64U | I64TruncSatF32S
        | I64TruncSatF32U | I64TruncSatF64S | I64TruncSatF64U => {
            out.push(0xFC);
            put_u32(out, sat_opcode(instr));
        }
        I32Extend8S => out.push(0xC0),
        I32Extend16S => out.push(0xC1),
        I64Extend8S => out.push(0xC2),
        I64Extend16S => out.push(0xC3),
        I64Extend32S => out.push(0xC4),
        _ => out.push(numeric_opcode(instr)),
    }
}

fn load_opcode(kind: LoadKind) -> u8 {
    match kind {
        LoadKind::I32 => 0x28,
        LoadKind::I64 => 0x29,
        LoadKind::F32 => 0x2A,
        LoadKind::F64 => 0x2B,
        LoadKind::I32x8S => 0x2C,
        LoadKind::I32x8U => 0x2D,
        LoadKind::I32x16S => 0x2E,
        LoadKind::I32x16U => 0x2F,
        LoadKind::I64x8S => 0x30,
        LoadKind::I64x8U => 0x31,
        LoadKind::I64x16S => 0x32,
        LoadKind::I64x16U => 0x33,
        LoadKind::I64x32S => 0x34,
        LoadKind::I64x32U => 0x35,
    }
}

fn store_opcode(kind: StoreKind) -> u8 {
    match kind {
        StoreKind::I32 => 0x36,
        StoreKind::I64 => 0x37,
        StoreKind::F32 => 0x38,
        StoreKind::F64 => 0x39,
        StoreKind::I32x8 => 0x3A,
        StoreKind::I32x16 => 0x3B,
        StoreKind::I64x8 => 0x3C,
        StoreKind::I64x16 => 0x3D,
        StoreKind::I64x32 => 0x3E,
    }
}

fn sat_opcode(instr: &Instr) -> u32 {
    match instr {
        Instr::I32TruncSatF32S => 0,
        Instr::I32TruncSatF32U => 1,
        Instr::I32TruncSatF64S => 2,
        Instr::I32TruncSatF64U => 3,
        Instr::I64TruncSatF32S => 4,
        Instr::I64TruncSatF32U => 5,
        Instr::I64TruncSatF64S => 6,
        _ => 7,
    }
}

fn numeric_opcode(instr: &Instr) -> u8 {
    use Instr::*;
    match instr {
        I32Eqz => 0x45,
        I32Eq => 0x46,
        I32Ne => 0x47,
        I32LtS => 0x48,
        I32LtU => 0x49,
        I32GtS => 0x4A,
        I32GtU => 0x4B,
        I32LeS => 0x4C,
        I32LeU => 0x4D,
        I32GeS => 0x4E,
        I32GeU => 0x4F,
        I64Eqz => 0x50,
        I64Eq => 0x51,
        I64Ne => 0x52,
        I64LtS => 0x53,
        I64LtU => 0x54,
        I64GtS => 0x55,
        I64GtU => 0x56,
        I64LeS => 0x57,
        I64LeU => 0x58,
        I64GeS => 0x59,
        I64GeU => 0x5A,
        F32Eq => 0x5B,
        F32Ne => 0x5C,
        F32Lt => 0x5D,
        F32Gt => 0x5E,
        F32Le => 0x5F,
        F32Ge => 0x60,
        F64Eq => 0x61,
        F64Ne => 0x62,
        F64Lt => 0x63,
        F64Gt => 0x64,
        F64Le => 0x65,
        F64Ge => 0x66,
        I32Clz => 0x67,
        I32Ctz => 0x68,
        I32Popcnt => 0x69,
        I32Add => 0x6A,
        I32Sub => 0x6B,
        I32Mul => 0x6C,
        I32DivS => 0x6D,
        I32DivU => 0x6E,
        I32RemS => 0x6F,
        I32RemU => 0x70,
        I32And => 0x71,
        I32Or => 0x72,
        I32Xor => 0x73,
        I32Shl => 0x74,
        I32ShrS => 0x75,
        I32ShrU => 0x76,
        I32Rotl => 0x77,
        I32Rotr => 0x78,
        I64Clz => 0x79,
        I64Ctz => 0x7A,
        I64Popcnt => 0x7B,
        I64Add => 0x7C,
        I64Sub => 0x7D,
        I64Mul => 0x7E,
        I64DivS => 0x7F,
        I64DivU => 0x80,
        I64RemS => 0x81,
        I64RemU => 0x82,
        I64And => 0x83,
        I64Or => 0x84,
        I64Xor => 0x85,
        I64Shl => 0x86,
        I64ShrS => 0x87,
        I64ShrU => 0x88,
        I64Rotl => 0x89,
        I64Rotr => 0x8A,
        F32Abs => 0x8B,
        F32Neg => 0x8C,
        F32Ceil => 0x8D,
        F32Floor => 0x8E,
        F32Trunc => 0x8F,
        F32Nearest => 0x90,
        F32Sqrt => 0x91,
        F32Add => 0x92,
        F32Sub => 0x93,
        F32Mul => 0x94,
        F32Div => 0x95,
        F32Min => 0x96,
        F32Max => 0x97,
        F32Copysign => 0x98,
        F64Abs => 0x99,
        F64Neg => 0x9A,
        F64Ceil => 0x9B,
        F64Floor => 0x9C,
        F64Trunc => 0x9D,
        F64Nearest => 0x9E,
        F64Sqrt => 0x9F,
        F64Add => 0xA0,
        F64Sub => 0xA1,
        F64Mul => 0xA2,
        F64Div => 0xA3,
        F64Min => 0xA4,
        F64Max => 0xA5,
        F64Copysign => 0xA6,
        I32WrapI64 => 0xA7,
        I32TruncF32S => 0xA8,
        I32TruncF32U => 0xA9,
        I32TruncF64S => 0xAA,
        I32TruncF64U => 0xAB,
        I64ExtendI32S => 0xAC,
        I64ExtendI32U => 0xAD,
        I64TruncF32S => 0xAE,
        I64TruncF32U => 0xAF,
        I64TruncF64S => 0xB0,
        I64TruncF64U => 0xB1,
        F32ConvertI32S => 0xB2,
        F32ConvertI32U => 0xB3,
        F32ConvertI64S => 0xB4,
        F32ConvertI64U => 0xB5,
        F32DemoteF64 => 0xB6,
        F64ConvertI32S => 0xB7,
        F64ConvertI32U => 0xB8,
        F64ConvertI64S => 0xB9,
        F64ConvertI64U => 0xBA,
        F64PromoteF32 => 0xBB,
        I32ReinterpretF32 => 0xBC,
        I64ReinterpretF64 => 0xBD,
        F32ReinterpretI32 => 0xBE,
        F64ReinterpretI64 => 0xBF,
        _ => 0x00,
    }
}
