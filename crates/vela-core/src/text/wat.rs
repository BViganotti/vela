//! WebAssembly text format parser for the subset used by Vela fixtures.
//!
//! Supported: modules, types, imports/exports, functions with named params and
//! locals, globals, memories, tables, data/element segments, start, and all
//! instructions implemented by the interpreter. Folded and flat syntax are
//! both accepted. Out-of-scope text features produce explicit errors.

use std::collections::HashMap;

use crate::error::{TextError, TextResult};
use crate::instr::{BrTable, Instr, LoadKind, MemArg, StoreKind};
use crate::module::{
    Data, DataMode, Element, ElementMode, Export, ExportDesc, Func, FuncBody, Global, Import,
    ImportDesc, Module,
};
use crate::text::sexpr::{self, Sexpr, SexprKind};
use crate::types::{
    BlockType, FuncType, GlobalType, Limits, MemoryType, TableType, ValType, MAX_PAGES,
};

struct WatParser {
    module: Module,
    type_names: HashMap<String, u32>,
    func_names: HashMap<String, u32>,
    table_names: HashMap<String, u32>,
    mem_names: HashMap<String, u32>,
    global_names: HashMap<String, u32>,
    func_total: u32,
    table_total: u32,
    mem_total: u32,
    global_total: u32,
    func_defined: bool,
    table_defined: bool,
    mem_defined: bool,
    global_defined: bool,
}

pub fn parse_module(src: &str) -> TextResult<Module> {
    let items = sexpr::parse_str(src)?;
    if items.len() != 1 {
        return Err(TextError::new(1, 1, "expected exactly one module"));
    }
    let module = items[0].list().ok_or_else(|| items[0].err("expected a module"))?;
    if module.first().and_then(|s| s.atom()) != Some("module") {
        return Err(items[0].err("expected (module ...)"));
    }
    let mut fields: &[Sexpr] = &module[1..];
    if let Some(first) = fields.first() {
        if first.atom().map(|a| a.starts_with('$')).unwrap_or(false) {
            fields = &fields[1..];
        }
    }
    WatParser::new().parse(fields)
}

fn parse_u32(s: &Sexpr) -> TextResult<u32> {
    let a = s.atom().ok_or_else(|| s.err("expected an integer"))?;
    parse_u32_str(a).ok_or_else(|| s.err(format!("invalid integer {a:?}")))
}

fn parse_u32_str(a: &str) -> Option<u32> {
    if let Some(hex) = a.strip_prefix("0x").or_else(|| a.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        a.parse::<u32>().ok()
    }
}

fn parse_i32(s: &Sexpr) -> TextResult<i32> {
    let a = s.atom().ok_or_else(|| s.err("expected i32 literal"))?;
    parse_i32_str(a).ok_or_else(|| s.err(format!("invalid i32 literal {a:?}")))
}

fn parse_i32_str(a: &str) -> Option<i32> {
    if let Some(hex) = a.strip_prefix("0x").or_else(|| a.strip_prefix("0X")) {
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            return Some(v as i32);
        }
    }
    if let Some(hex) = a.strip_prefix("-0x") {
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            return Some((v as i32).wrapping_neg());
        }
    }
    a.parse::<i32>().ok()
}

fn parse_i64(s: &Sexpr) -> TextResult<i64> {
    let a = s.atom().ok_or_else(|| s.err("expected i64 literal"))?;
    parse_i64_str(a).ok_or_else(|| s.err(format!("invalid i64 literal {a:?}")))
}

fn parse_i64_str(a: &str) -> Option<i64> {
    if let Some(hex) = a.strip_prefix("0x").or_else(|| a.strip_prefix("0X")) {
        if let Ok(v) = u64::from_str_radix(hex, 16) {
            return Some(v as i64);
        }
    }
    if let Some(hex) = a.strip_prefix("-0x") {
        if let Ok(v) = u64::from_str_radix(hex, 16) {
            return Some((v as i64).wrapping_neg());
        }
    }
    a.parse::<i64>().ok()
}

fn parse_f32(s: &Sexpr) -> TextResult<u32> {
    let a = s.atom().ok_or_else(|| s.err("expected f32 literal"))?;
    parse_f32_str(a)
        .map(|v| v.to_bits())
        .ok_or_else(|| s.err(format!("invalid f32 literal {a:?}")))
}

fn parse_f32_str(a: &str) -> Option<f32> {
    if a == "nan" || a == "+nan" {
        return Some(f32::from_bits(0x7fc0_0000));
    }
    if a == "-nan" {
        return Some(f32::from_bits(0xffc0_0000));
    }
    if let Some(bits) = a.strip_prefix("nan:0x") {
        return u32::from_str_radix(bits, 16)
            .ok()
            .map(|payload| f32::from_bits(0x7f80_0000 | (payload & 0x007f_ffff)));
    }
    if let Some(bits) = a.strip_prefix("-nan:0x") {
        return u32::from_str_radix(bits, 16)
            .ok()
            .map(|payload| f32::from_bits(0xff80_0000 | (payload & 0x007f_ffff)));
    }
    if a == "inf" || a == "+inf" {
        return Some(f32::INFINITY);
    }
    if a == "-inf" {
        return Some(f32::NEG_INFINITY);
    }
    if let Some(hex) = a.strip_prefix("0x").or_else(|| a.strip_prefix("-0x")) {
        let negative = a.starts_with('-');
        let rate = if negative { -1.0f64 } else { 1.0f64 };
        return parse_hex_float(hex).map(|v| (v * rate) as f32);
    }
    a.parse::<f32>().ok()
}

fn parse_f64(s: &Sexpr) -> TextResult<u64> {
    let a = s.atom().ok_or_else(|| s.err("expected f64 literal"))?;
    parse_f64_str(a)
        .map(|v| v.to_bits())
        .ok_or_else(|| s.err(format!("invalid f64 literal {a:?}")))
}

fn parse_f64_str(a: &str) -> Option<f64> {
    if a == "nan" || a == "+nan" {
        return Some(f64::from_bits(0x7ff8_0000_0000_0000));
    }
    if a == "-nan" {
        return Some(f64::from_bits(0xfff8_0000_0000_0000));
    }
    if let Some(bits) = a.strip_prefix("nan:0x") {
        return u64::from_str_radix(bits, 16)
            .ok()
            .map(|payload| f64::from_bits(0x7ff0_0000_0000_0000 | (payload & 0x000f_ffff_ffff_ffff)));
    }
    if let Some(bits) = a.strip_prefix("-nan:0x") {
        return u64::from_str_radix(bits, 16)
            .ok()
            .map(|payload| f64::from_bits(0xfff0_0000_0000_0000 | (payload & 0x000f_ffff_ffff_ffff)));
    }
    if a == "inf" || a == "+inf" {
        return Some(f64::INFINITY);
    }
    if a == "-inf" {
        return Some(f64::NEG_INFINITY);
    }
    if let Some(hex) = a.strip_prefix("0x").or_else(|| a.strip_prefix("-0x")) {
        let negative = a.starts_with('-');
        let rate = if negative { -1.0f64 } else { 1.0f64 };
        return parse_hex_float(hex).map(|v| v * rate);
    }
    a.parse::<f64>().ok()
}

fn parse_hex_float(hex: &str) -> Option<f64> {
    let (mantissa, exponent) = match hex.split_once(['p', 'P']) {
        Some((m, e)) => (m, e.parse::<i32>().ok()?),
        None => (hex, 0),
    };
    let (int_part, frac_part) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    let mut value = 0.0f64;
    for c in int_part.chars() {
        value = value * 16.0 + c.to_digit(16)? as f64;
    }
    let mut scale = 1.0f64 / 16.0;
    for c in frac_part.chars() {
        value += c.to_digit(16)? as f64 * scale;
        scale /= 16.0;
    }
    Some(value * 2f64.powi(exponent))
}

fn parse_valtype(s: &Sexpr) -> TextResult<ValType> {
    let a = s.atom().ok_or_else(|| s.err("expected a value type"))?;
    ValType::from_name(a).ok_or_else(|| s.err(format!("unknown value type {a:?}")))
}

fn parse_ref_type(s: &Sexpr) -> TextResult<ValType> {
    let a = s.atom().ok_or_else(|| s.err("expected a reference type"))?;
    match a {
        "funcref" | "func" => Ok(ValType::FuncRef),
        "externref" | "extern" => Ok(ValType::ExternRef),
        other => Err(s.err(format!("unknown reference type {other:?}"))),
    }
}

impl WatParser {
    fn new() -> Self {
        WatParser {
            module: Module::default(),
            type_names: HashMap::new(),
            func_names: HashMap::new(),
            table_names: HashMap::new(),
            mem_names: HashMap::new(),
            global_names: HashMap::new(),
            func_total: 0,
            table_total: 0,
            mem_total: 0,
            global_total: 0,
            func_defined: false,
            table_defined: false,
            mem_defined: false,
            global_defined: false,
        }
    }

    fn parse(mut self, fields: &[Sexpr]) -> TextResult<Module> {
        self.parse_types(fields)?;
        self.scan_names(fields)?;
        for field in fields {
            let list = match field.list() {
                Some(l) if !l.is_empty() => l,
                _ => return Err(field.err("expected a module field")),
            };
            let head = list[0]
                .atom()
                .ok_or_else(|| list[0].err("expected a field name"))?;
            match head {
                "type" => {}
                "import" => self.parse_import(field)?,
                "func" => self.parse_func_field(field)?,
                "table" => self.parse_table_field(field)?,
                "memory" => self.parse_memory_field(field)?,
                "global" => self.parse_global_field(field)?,
                "export" => self.parse_export_field(field)?,
                "start" => {
                    if list.len() != 2 {
                        return Err(field.err("malformed start"));
                    }
                    let idx = self.resolve_func(&list[1])?;
                    self.module.start = Some(idx);
                }
                "data" => self.parse_data_field(field)?,
                "elem" => self.parse_elem_field(field)?,
                other => {
                    return Err(field.err(format!("unsupported module field {other:?}")))
                }
            }
        }
        if !self.module.datas.is_empty() {
            self.module.data_count = Some(self.module.datas.len() as u32);
        }
        Ok(self.module)
    }

    fn parse_types(&mut self, fields: &[Sexpr]) -> TextResult<()> {
        for field in fields {
            let list = match field.list() {
                Some(l) if !l.is_empty() => l,
                _ => continue,
            };
            if list[0].atom() != Some("type") {
                continue;
            }
            let mut i = 1;
            let mut name = None;
            if let Some(a) = list.get(i).and_then(|s| s.atom()) {
                if a.starts_with('$') {
                    name = Some(a.to_string());
                    i += 1;
                }
            }
            let fty = list.get(i).ok_or_else(|| field.err("type is missing a function type"))?;
            let ft_list = fty.list().ok_or_else(|| fty.err("expected (func ...)"))?;
            if ft_list.first().and_then(|s| s.atom()) != Some("func") {
                return Err(fty.err("only function types are supported"));
            }
            let ft = self.parse_func_type(&ft_list[1..])?;
            let idx = self.module.types.len() as u32;
            self.module.types.push(ft);
            if let Some(name) = name {
                self.type_names.insert(name, idx);
            }
        }
        Ok(())
    }

    fn parse_func_type(&self, items: &[Sexpr]) -> TextResult<FuncType> {
        let mut params = Vec::new();
        let mut results = Vec::new();
        let mut i = 0;
        while i < items.len() {
            let head = items[i]
                .list()
                .and_then(|l| l.first())
                .and_then(|s| s.atom());
            match head {
                Some("param") => {
                    let list = items[i].list().unwrap();
                    let mut j = 1;
                    if list.get(j).and_then(|s| s.atom()).map(|a| a.starts_with('$')).unwrap_or(false) {
                        j += 1;
                    }
                    while j < list.len() {
                        params.push(parse_valtype(&list[j])?);
                        j += 1;
                    }
                }
                Some("result") => {
                    let list = items[i].list().unwrap();
                    for s in &list[1..] {
                        results.push(parse_valtype(s)?);
                    }
                }
                _ => break,
            }
            i += 1;
        }
        Ok(FuncType { params, results })
    }

    fn scan_names(&mut self, fields: &[Sexpr]) -> TextResult<()> {
        for field in fields {
            let list = match field.list() {
                Some(l) if !l.is_empty() => l,
                _ => continue,
            };
            let head = match list[0].atom() {
                Some(h) => h,
                None => continue,
            };
            match head {
                "func" | "table" | "memory" | "global" => {
                    let imp = list.iter().any(|s| s.head() == Some("import"));
                    let name = list
                        .get(1)
                        .and_then(|s| s.atom())
                        .filter(|a| a.starts_with('$'))
                        .map(|a| a.to_string());
                    match head {
                        "func" => {
                            if imp {
                                if self.func_defined {
                                    return Err(field.err("import after function definition"));
                                }
                                if let Some(n) = name {
                                    self.func_names.insert(n, self.func_total);
                                }
                                self.func_total += 1;
                            } else {
                                self.func_defined = true;
                                if let Some(n) = name {
                                    self.func_names.insert(n, self.func_total);
                                }
                                self.func_total += 1;
                            }
                        }
                        "table" => {
                            if imp {
                                if self.table_defined {
                                    return Err(field.err("import after table definition"));
                                }
                                if let Some(n) = name {
                                    self.table_names.insert(n, self.table_total);
                                }
                                self.table_total += 1;
                            } else {
                                self.table_defined = true;
                                if let Some(n) = name {
                                    self.table_names.insert(n, self.table_total);
                                }
                                self.table_total += 1;
                            }
                        }
                        "memory" => {
                            if imp {
                                if self.mem_defined {
                                    return Err(field.err("import after memory definition"));
                                }
                                if let Some(n) = name {
                                    self.mem_names.insert(n, self.mem_total);
                                }
                                self.mem_total += 1;
                            } else {
                                self.mem_defined = true;
                                if let Some(n) = name {
                                    self.mem_names.insert(n, self.mem_total);
                                }
                                self.mem_total += 1;
                            }
                        }
                        _ => {
                            if imp {
                                if self.global_defined {
                                    return Err(field.err("import after global definition"));
                                }
                                if let Some(n) = name {
                                    self.global_names.insert(n, self.global_total);
                                }
                                self.global_total += 1;
                            } else {
                                self.global_defined = true;
                                if let Some(n) = name {
                                    self.global_names.insert(n, self.global_total);
                                }
                                self.global_total += 1;
                            }
                        }
                    }
                }
                "import" => {
                    if list.len() != 4 {
                        return Err(field.err("malformed import"));
                    }
                    let desc = list[3]
                        .list()
                        .ok_or_else(|| list[3].err("malformed import descriptor"))?;
                    let kind = desc
                        .first()
                        .and_then(|s| s.atom())
                        .ok_or_else(|| list[3].err("malformed import descriptor"))?;
                    let name = desc
                        .get(1)
                        .and_then(|s| s.atom())
                        .filter(|a| a.starts_with('$'))
                        .map(|a| a.to_string());
                    match kind {
                        "func" => {
                            if self.func_defined {
                                return Err(field.err("import after function definition"));
                            }
                            if let Some(n) = name {
                                self.func_names.insert(n, self.func_total);
                            }
                            self.func_total += 1;
                        }
                        "table" => {
                            if self.table_defined {
                                return Err(field.err("import after table definition"));
                            }
                            if let Some(n) = name {
                                self.table_names.insert(n, self.table_total);
                            }
                            self.table_total += 1;
                        }
                        "memory" => {
                            if self.mem_defined {
                                return Err(field.err("import after memory definition"));
                            }
                            if let Some(n) = name {
                                self.mem_names.insert(n, self.mem_total);
                            }
                            self.mem_total += 1;
                        }
                        "global" => {
                            if self.global_defined {
                                return Err(field.err("import after global definition"));
                            }
                            if let Some(n) = name {
                                self.global_names.insert(n, self.global_total);
                            }
                            self.global_total += 1;
                        }
                        _ => return Err(field.err("unknown import kind")),
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn resolve_func(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a function"))?;
        if a.starts_with('$') {
            self.func_names
                .get(a)
                .copied()
                .ok_or_else(|| s.err(format!("unknown function {a}")))
        } else {
            parse_u32_str(a).ok_or_else(|| s.err(format!("invalid function index {a:?}")))
        }
    }

    fn resolve_type(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a type"))?;
        if a.starts_with('$') {
            self.type_names
                .get(a)
                .copied()
                .ok_or_else(|| s.err(format!("unknown type {a}")))
        } else {
            parse_u32_str(a).ok_or_else(|| s.err(format!("invalid type index {a:?}")))
        }
    }

    fn resolve_table(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a table"))?;
        if a.starts_with('$') {
            self.table_names
                .get(a)
                .copied()
                .ok_or_else(|| s.err(format!("unknown table {a}")))
        } else {
            parse_u32_str(a).ok_or_else(|| s.err(format!("invalid table index {a:?}")))
        }
    }

    fn resolve_memory(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a memory"))?;
        if a.starts_with('$') {
            self.mem_names
                .get(a)
                .copied()
                .ok_or_else(|| s.err(format!("unknown memory {a}")))
        } else {
            parse_u32_str(a).ok_or_else(|| s.err(format!("invalid memory index {a:?}")))
        }
    }

    fn resolve_global(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a global"))?;
        if a.starts_with('$') {
            self.global_names
                .get(a)
                .copied()
                .ok_or_else(|| s.err(format!("unknown global {a}")))
        } else {
            parse_u32_str(a).ok_or_else(|| s.err(format!("invalid global index {a:?}")))
        }
    }

    fn dedupe_type(&mut self, ft: FuncType) -> u32 {
        if let Some(i) = self.module.types.iter().position(|t| *t == ft) {
            return i as u32;
        }
        let idx = self.module.types.len() as u32;
        self.module.types.push(ft);
        idx
    }

    fn parse_import(&mut self, field: &Sexpr) -> TextResult<()> {
        let list = field.list().unwrap();
        if list.len() != 4 {
            return Err(field.err("malformed import"));
        }
        let module = list[1]
            .bytes()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .ok_or_else(|| list[1].err("expected an import module string"))?;
        let name = list[2]
            .bytes()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .ok_or_else(|| list[2].err("expected an import name string"))?;
        let desc = list[3]
            .list()
            .ok_or_else(|| list[3].err("malformed import descriptor"))?;
        let kind = desc
            .first()
            .and_then(|s| s.atom())
            .ok_or_else(|| list[3].err("malformed import descriptor"))?;
        let mut i = 1;
        if desc.get(i).and_then(|s| s.atom()).map(|a| a.starts_with('$')).unwrap_or(false) {
            i += 1;
        }
        let desc = match kind {
            "func" => {
                let ty = self.parse_type_use(&desc[i..])?;
                ImportDesc::Func(ty)
            }
            "table" => {
                let (limits, elem) = parse_table_limits(&desc[i..])?;
                ImportDesc::Table(TableType { elem, limits })
            }
            "memory" => {
                let limits = parse_limits(&desc[i..], MAX_PAGES)?;
                ImportDesc::Memory(MemoryType { limits })
            }
            "global" => {
                let g = desc.get(i).ok_or_else(|| list[3].err("missing global type"))?;
                let ty = parse_global_type(g)?;
                ImportDesc::Global(ty)
            }
            _ => return Err(field.err("unknown import kind")),
        };
        self.module.imports.push(Import { module, name, desc });
        Ok(())
    }

    /// Parse `(type $t)? (param...)*(result...)*`, returning the type index.
    fn parse_type_use(&mut self, items: &[Sexpr]) -> TextResult<u32> {
        let mut explicit = None;
        let mut params: Vec<ValType> = Vec::new();
        let mut results: Vec<ValType> = Vec::new();
        let mut i = 0;
        while i < items.len() {
            let head = items[i].list().and_then(|l| l.first()).and_then(|s| s.atom());
            match head {
                Some("type") => {
                    let list = items[i].list().unwrap();
                    let idx = list.get(1).ok_or_else(|| items[i].err("missing type index"))?;
                    explicit = Some(self.resolve_type(idx)?);
                }
                Some("param") => {
                    let list = items[i].list().unwrap();
                    let mut j = 1;
                    if list.get(j).and_then(|s| s.atom()).map(|a| a.starts_with('$')).unwrap_or(false) {
                        j += 1;
                    }
                    while j < list.len() {
                        params.push(parse_valtype(&list[j])?);
                        j += 1;
                    }
                }
                Some("result") => {
                    let list = items[i].list().unwrap();
                    for s in &list[1..] {
                        results.push(parse_valtype(s)?);
                    }
                }
                _ => break,
            }
            i += 1;
        }
        if i != items.len() {
            return Err(items[i].err("unexpected token in type use"));
        }
        Ok(match explicit {
            Some(idx) => idx,
            None => self.dedupe_type(FuncType { params, results }),
        })
    }

    fn parse_func_field(&mut self, field: &Sexpr) -> TextResult<()> {
        let list = field.list().unwrap();
        let mut i = 1;
        if list.get(i).and_then(|s| s.atom()).map(|a| a.starts_with('$')).unwrap_or(false) {
            i += 1;
        }
        let mut exports = Vec::new();
        let mut import = None;
        let mut local_decls: Vec<(Option<String>, Vec<ValType>)> = Vec::new();
        let mut params: Vec<ValType> = Vec::new();
        let mut param_names: Vec<String> = Vec::new();
        let mut results: Vec<ValType> = Vec::new();
        let mut explicit_type: Option<u32> = None;
        let mut results_given = false;

        while i < list.len() {
            let head = list[i].list().and_then(|l| l.first()).and_then(|s| s.atom());
            match head {
                Some("export") => {
                    let inner = list[i].list().unwrap();
                    let name = inner
                        .get(1)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("export needs a name"))?;
                    exports.push(name);
                }
                Some("import") => {
                    let inner = list[i].list().unwrap();
                    let m = inner
                        .get(1)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("import needs a module"))?;
                    let n = inner
                        .get(2)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("import needs a name"))?;
                    import = Some((m, n));
                }
                Some("type") => {
                    let inner = list[i].list().unwrap();
                    let idx = inner.get(1).ok_or_else(|| list[i].err("missing type"))?;
                    explicit_type = Some(self.resolve_type(idx)?);
                }
                Some("param") => {
                    let inner = list[i].list().unwrap();
                    let mut j = 1;
                    if let Some(a) = inner.get(j).and_then(|s| s.atom()) {
                        if a.starts_with('$') {
                            param_names.push(a.to_string());
                            j += 1;
                        } else {
                            param_names.push(String::new());
                        }
                    }
                    let mut count = 0;
                    while j < inner.len() {
                        params.push(parse_valtype(&inner[j])?);
                        j += 1;
                        count += 1;
                    }
                    if count == 0 {
                        return Err(list[i].err("empty param group"));
                    }
                    for _ in 1..count {
                        param_names.push(String::new());
                    }
                }
                Some("result") => {
                    let inner = list[i].list().unwrap();
                    results_given = true;
                    for s in &inner[1..] {
                        results.push(parse_valtype(s)?);
                    }
                }
                Some("local") => {
                    let inner = list[i].list().unwrap();
                    let mut j = 1;
                    let mut name = None;
                    if let Some(a) = inner.get(j).and_then(|s| s.atom()) {
                        if a.starts_with('$') {
                            name = Some(a.to_string());
                            j += 1;
                        }
                    }
                    let mut tys = Vec::new();
                    while j < inner.len() {
                        tys.push(parse_valtype(&inner[j])?);
                        j += 1;
                    }
                    if tys.is_empty() {
                        return Err(list[i].err("empty local group"));
                    }
                    if name.is_some() && tys.len() != 1 {
                        return Err(list[i].err("a named local must have exactly one type"));
                    }
                    local_decls.push((name, tys));
                }
                _ => break,
            }
            i += 1;
        }

        let ft = match explicit_type {
            Some(t) => {
                let declared = self
                    .module
                    .types
                    .get(t as usize)
                    .ok_or_else(|| field.err("unknown type"))?
                    .clone();
                if !params.is_empty() && params != declared.params {
                    return Err(field.err("params do not match the referenced type"));
                }
                if results_given && results != declared.results {
                    return Err(field.err("results do not match the referenced type"));
                }
                if params.is_empty() {
                    params = declared.params.clone();
                    param_names.resize(params.len(), String::new());
                }
                t
            }
            None => self.dedupe_type(FuncType { params: params.clone(), results: results.clone() }),
        };

        if let Some((m, n)) = import {
            if i < list.len() {
                return Err(field.err("imported function cannot have a body"));
            }
            if !local_decls.is_empty() {
                return Err(field.err("imported function cannot declare locals"));
            }
            let idx = self.module.imported_funcs() as u32;
            self.module.imports.push(Import { module: m, name: n, desc: ImportDesc::Func(ft) });
            for name in exports {
                self.module.exports.push(Export { name, desc: ExportDesc::Func(idx) });
            }
            return Ok(());
        }

        let mut local_types = Vec::new();
        let mut local_names = HashMap::new();
        for (idx, name) in param_names.iter().enumerate() {
            if !name.is_empty() {
                local_names.insert(name.clone(), idx as u32);
            }
        }
        for (name, tys) in local_decls {
            let base = params.len() + local_types.len();
            if let (Some(name), Some(ty)) = (&name, tys.first()) {
                let _ = ty;
                local_names.insert(name.clone(), base as u32);
            }
            local_types.extend(tys);
        }

        let current = self.module.imported_funcs() as u32 + self.module.funcs.len() as u32;
        let body_tokens = &list[i..];
        let mut local_all = params.clone();
        local_all.extend(local_types.iter().copied());
        let mut ctx = InstrCtx {
            type_names: self.type_names.clone(),
            func_names: self.func_names.clone(),
            table_names: self.table_names.clone(),
            mem_names: self.mem_names.clone(),
            global_names: self.global_names.clone(),
            type_base: self.module.types.len() as u32,
            extra_types: Vec::new(),
            locals: local_all,
            local_names,
            labels: Vec::new(),
        };
        let mut pos = 0;
        let mut body = ctx.parse_seq(body_tokens, &mut pos, &[])?;
        body.push(Instr::End);
        if pos != body_tokens.len() {
            return Err(field.err("trailing tokens in function body"));
        }
        self.module.types.extend(ctx.extra_types);
        self.module.funcs.push(Func {
            type_idx: ft,
            body: FuncBody { locals: local_types, body },
        });
        for name in exports {
            self.module.exports.push(Export { name, desc: ExportDesc::Func(current) });
        }
        Ok(())
    }

    fn parse_table_field(&mut self, field: &Sexpr) -> TextResult<()> {
        let list = field.list().unwrap();
        let mut i = 1;
        if list.get(i).and_then(|s| s.atom()).map(|a| a.starts_with('$')).unwrap_or(false) {
            i += 1;
        }
        let mut exports = Vec::new();
        let mut import = None;
        let mut limits = None;
        let mut elem_type = ValType::FuncRef;
        let mut inline_elem: Option<Vec<Vec<Instr>>> = None;
        while i < list.len() {
            match list[i].list().and_then(|l| l.first()).and_then(|s| s.atom()) {
                Some("export") => {
                    let inner = list[i].list().unwrap();
                    let name = inner
                        .get(1)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("export needs a name"))?;
                    exports.push(name);
                    i += 1;
                }
                Some("import") => {
                    let inner = list[i].list().unwrap();
                    let m = inner
                        .get(1)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("import needs a module"))?;
                    let n = inner
                        .get(2)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("import needs a name"))?;
                    import = Some((m, n));
                    i += 1;
                }
                Some("elem") => {
                    let inner = list[i].list().unwrap();
                    let mut items = Vec::new();
                    for s in &inner[1..] {
                        items.push(self.const_ref_expr(s)?);
                    }
                    limits = Some(Limits::new(items.len() as u32, Some(items.len() as u32)));
                    inline_elem = Some(items);
                    i += 1;
                }
                _ => {
                    if let Some(a) = list[i].atom() {
                        if let Ok(t) = parse_ref_type(&list[i]) {
                            elem_type = t;
                            i += 1;
                            continue;
                        }
                        if parse_u32_str(a).is_some() {
                            let min = parse_u32(&list[i])?;
                            i += 1;
                            let max = if let Some(next) = list.get(i) {
                                if let Some(a) = next.atom() {
                                    if let Some(v) = parse_u32_str(a) {
                                        i += 1;
                                        Some(v)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            limits = Some(Limits::new(min, max));
                            continue;
                        }
                    }
                    return Err(list[i].err("malformed table"));
                }
            }
        }
        let limits = limits.ok_or_else(|| field.err("table is missing limits"))?;
        let tt = TableType { elem: elem_type, limits };
        if let Some((m, n)) = import {
            let current = self.imported_tables();
            self.module.imports.push(Import { module: m, name: n, desc: ImportDesc::Table(tt) });
            for name in exports {
                self.module.exports.push(Export { name, desc: ExportDesc::Table(current) });
            }
        } else {
            let current = self.module.tables.len() as u32 + self.imported_tables();
            self.module.tables.push(tt);
            if let Some(items) = inline_elem {
                self.module.elems.push(Element {
                    mode: ElementMode::Active { table: current, offset: vec![Instr::I32Const(0), Instr::End] },
                    elem_type,
                    init: items,
                });
            }
            for name in exports {
                self.module.exports.push(Export { name, desc: ExportDesc::Table(current) });
            }
        }
        Ok(())
    }

    fn imported_tables(&self) -> u32 {
        self.module
            .imports
            .iter()
            .filter(|i| matches!(i.desc, ImportDesc::Table(_)))
            .count() as u32
    }

    fn imported_memories(&self) -> u32 {
        self.module
            .imports
            .iter()
            .filter(|i| matches!(i.desc, ImportDesc::Memory(_)))
            .count() as u32
    }

    fn imported_globals(&self) -> u32 {
        self.module
            .imports
            .iter()
            .filter(|i| matches!(i.desc, ImportDesc::Global(_)))
            .count() as u32
    }

    fn const_ref_expr(&mut self, s: &Sexpr) -> TextResult<Vec<Instr>> {
        let ctx = InstrCtx {
            type_names: self.type_names.clone(),
            func_names: self.func_names.clone(),
            table_names: self.table_names.clone(),
            mem_names: self.mem_names.clone(),
            global_names: self.global_names.clone(),
            type_base: self.module.types.len() as u32,
            extra_types: Vec::new(),
            locals: Vec::new(),
            local_names: HashMap::new(),
            labels: Vec::new(),
        };
        let mut ctx = ctx;
        let mut out = Vec::new();
        match &s.kind {
            SexprKind::Atom(a) if a.starts_with('$') => {
                out.push(Instr::RefFunc(self.resolve_func(s)?));
            }
            SexprKind::Atom(a) => {
                if let Some(idx) = parse_u32_str(a) {
                    out.push(Instr::RefFunc(idx));
                } else {
                    return Err(s.err("expected a function reference"));
                }
            }
            SexprKind::List(_) => {
                out.extend(ctx.parse_folded(s)?);
            }
            _ => return Err(s.err("expected a reference expression")),
        }
        out.push(Instr::End);
        Ok(out)
    }

    fn parse_memory_field(&mut self, field: &Sexpr) -> TextResult<()> {
        let list = field.list().unwrap();
        let mut i = 1;
        if list.get(i).and_then(|s| s.atom()).map(|a| a.starts_with('$')).unwrap_or(false) {
            i += 1;
        }
        let mut exports = Vec::new();
        let mut import = None;
        while i < list.len() {
            match list[i].list().and_then(|l| l.first()).and_then(|s| s.atom()) {
                Some("export") => {
                    let inner = list[i].list().unwrap();
                    let name = inner
                        .get(1)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("export needs a name"))?;
                    exports.push(name);
                    i += 1;
                }
                Some("import") => {
                    let inner = list[i].list().unwrap();
                    let m = inner
                        .get(1)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("import needs a module"))?;
                    let n = inner
                        .get(2)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("import needs a name"))?;
                    import = Some((m, n));
                    i += 1;
                }
                _ => break,
            }
        }
        let limits = parse_limits(&list[i..], MAX_PAGES)?;
        let mt = MemoryType { limits };
        if let Some((m, n)) = import {
            let current = self.imported_memories();
            self.module.imports.push(Import { module: m, name: n, desc: ImportDesc::Memory(mt) });
            for name in exports {
                self.module.exports.push(Export { name, desc: ExportDesc::Memory(current) });
            }
        } else {
            let current = self.module.memories.len() as u32 + self.imported_memories();
            self.module.memories.push(mt);
            for name in exports {
                self.module.exports.push(Export { name, desc: ExportDesc::Memory(current) });
            }
        }
        Ok(())
    }

    fn parse_global_field(&mut self, field: &Sexpr) -> TextResult<()> {
        let list = field.list().unwrap();
        let mut i = 1;
        if list.get(i).and_then(|s| s.atom()).map(|a| a.starts_with('$')).unwrap_or(false) {
            i += 1;
        }
        let mut exports = Vec::new();
        let mut import = None;
        while i < list.len() {
            match list[i].list().and_then(|l| l.first()).and_then(|s| s.atom()) {
                Some("export") => {
                    let inner = list[i].list().unwrap();
                    let name = inner
                        .get(1)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("export needs a name"))?;
                    exports.push(name);
                    i += 1;
                }
                Some("import") => {
                    let inner = list[i].list().unwrap();
                    let m = inner
                        .get(1)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("import needs a module"))?;
                    let n = inner
                        .get(2)
                        .and_then(|s| s.bytes())
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .ok_or_else(|| list[i].err("import needs a name"))?;
                    import = Some((m, n));
                    i += 1;
                }
                _ => break,
            }
        }
        let ty = parse_global_type(
            list.get(i).ok_or_else(|| field.err("global is missing a type"))?,
        )?;
        i += 1;
        if let Some((m, n)) = import {
            let current = self.imported_globals();
            self.module.imports.push(Import { module: m, name: n, desc: ImportDesc::Global(ty) });
            for name in exports {
                self.module.exports.push(Export { name, desc: ExportDesc::Global(current) });
            }
            return Ok(());
        }
        let init = self.constant_body(&list[i..], field)?;
        let current = self.module.globals.len() as u32 + self.imported_globals();
        self.module.globals.push(Global { ty, init });
        for name in exports {
            self.module.exports.push(Export { name, desc: ExportDesc::Global(current) });
        }
        Ok(())
    }

    fn constant_body(&mut self, tokens: &[Sexpr], field: &Sexpr) -> TextResult<Vec<Instr>> {
        let mut ctx = InstrCtx {
            type_names: self.type_names.clone(),
            func_names: self.func_names.clone(),
            table_names: self.table_names.clone(),
            mem_names: self.mem_names.clone(),
            global_names: self.global_names.clone(),
            type_base: self.module.types.len() as u32,
            extra_types: Vec::new(),
            locals: Vec::new(),
            local_names: HashMap::new(),
            labels: Vec::new(),
        };
        let mut pos = 0;
        let mut out = ctx.parse_seq(tokens, &mut pos, &[])?;
        out.push(Instr::End);
        self.module.types.extend(ctx.extra_types);
        if pos != tokens.len() {
            return Err(field.err("trailing tokens in constant expression"));
        }
        Ok(out)
    }

    fn parse_export_field(&mut self, field: &Sexpr) -> TextResult<()> {
        let list = field.list().unwrap();
        if list.len() != 3 {
            return Err(field.err("malformed export"));
        }
        let name = list[1]
            .bytes()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .ok_or_else(|| list[1].err("export needs a name string"))?;
        let desc = list[2].list().ok_or_else(|| list[2].err("malformed export descriptor"))?;
        let kind = desc
            .first()
            .and_then(|s| s.atom())
            .ok_or_else(|| list[2].err("malformed export descriptor"))?;
        let idx = desc.get(1).ok_or_else(|| list[2].err("missing export index"))?;
        let desc = match kind {
            "func" => ExportDesc::Func(self.resolve_func(idx)?),
            "table" => ExportDesc::Table(self.resolve_table(idx)?),
            "memory" => ExportDesc::Memory(self.resolve_memory(idx)?),
            "global" => ExportDesc::Global(self.resolve_global(idx)?),
            _ => return Err(list[2].err("unknown export kind")),
        };
        self.module.exports.push(Export { name, desc });
        Ok(())
    }

    fn parse_data_field(&mut self, field: &Sexpr) -> TextResult<()> {
        let list = field.list().unwrap();
        let mut i = 1;
        let mut memory = 0u32;
        let mut offset = None;
        if let Some(inner) = list.get(i).and_then(|s| s.list()) {
            if inner.first().and_then(|s| s.atom()) == Some("memory") {
                memory = self.resolve_memory(&inner[1])?;
                i += 1;
            }
        }
        if let Some(inner) = list.get(i).and_then(|s| s.list()) {
            if inner.first().and_then(|s| s.atom()) == Some("offset") {
                offset = Some(self.constant_body(&inner[1..], &list[i])?);
                i += 1;
            } else if is_const_start(&inner[0]) {
                offset = Some(self.constant_body(std::slice::from_ref(&list[i]), &list[i])?);
                i += 1;
            }
        }
        let mut bytes = Vec::new();
        while i < list.len() {
            let b = list[i]
                .bytes()
                .ok_or_else(|| list[i].err("expected a data string"))?;
            bytes.extend_from_slice(b);
            i += 1;
        }
        let mode = match offset {
            Some(offset) => DataMode::Active { memory, offset },
            None => DataMode::Passive,
        };
        self.module.datas.push(Data { mode, bytes });
        Ok(())
    }

    fn parse_elem_field(&mut self, field: &Sexpr) -> TextResult<()> {
        let list = field.list().unwrap();
        let mut i = 1;
        let mut table = 0u32;
        let mut declared = false;
        let mut offset = None;
        let mut elem_type = ValType::FuncRef;
        if let Some(inner) = list.get(i).and_then(|s| s.list()) {
            if inner.first().and_then(|s| s.atom()) == Some("table") {
                table = self.resolve_table(&inner[1])?;
                i += 1;
            }
        }
        if list.get(i).and_then(|s| s.atom()) == Some("declare") {
            declared = true;
            i += 1;
            if list.get(i).and_then(|s| s.atom()) == Some("func") {
                i += 1;
            }
        } else if list.get(i).and_then(|s| s.atom()) == Some("func") {
            i += 1;
        } else if let Some(inner) = list.get(i).and_then(|s| s.list()) {
            if inner.first().and_then(|s| s.atom()) == Some("offset") {
                offset = Some(self.constant_body(&inner[1..], &list[i])?);
                i += 1;
            } else if is_const_start(&inner[0]) {
                offset = Some(self.constant_body(std::slice::from_ref(&list[i]), &list[i])?);
                i += 1;
            } else if let Some(a) = inner.first().and_then(|s| s.atom()) {
                if a == "externref" || a == "funcref" {
                    elem_type = parse_ref_type(&inner[0])?;
                    i += 1;
                }
            }
        }
        if let Some(s) = list.get(i) {
            if let Some(a) = s.atom() {
                if a == "func" {
                    i += 1;
                } else if a == "externref" || a == "funcref" {
                    elem_type = parse_ref_type(s)?;
                    i += 1;
                }
            }
        }
        let mut items = Vec::new();
        while i < list.len() {
            items.push(self.const_ref_expr(&list[i])?);
            i += 1;
        }
        let mode = if declared {
            ElementMode::Declarative
        } else {
            match offset {
                Some(offset) => ElementMode::Active { table, offset },
                None => ElementMode::Passive,
            }
        };
        self.module.elems.push(Element { mode, elem_type, init: items });
        Ok(())
    }
}

fn is_const_start(s: &Sexpr) -> bool {
    matches!(
        s.atom(),
        Some("i32.const")
            | Some("i64.const")
            | Some("f32.const")
            | Some("f64.const")
            | Some("global.get")
    )
}

fn parse_limits(items: &[Sexpr], hard_max: u32) -> TextResult<Limits> {
    let mut nums = Vec::new();
    for s in items {
        let a = s.atom().ok_or_else(|| s.err("expected limits"))?;
        if a == "shared" {
            return Err(s.err("shared memories are not supported"));
        }
        let v = parse_u32_str(a).ok_or_else(|| s.err(format!("invalid limit {a:?}")))?;
        nums.push(v);
    }
    if nums.is_empty() || nums.len() > 2 {
        return Err(
            items
                .first()
                .map(|s| s.err("malformed limits"))
                .unwrap_or_else(|| TextError::new(1, 1, "malformed limits")),
        );
    }
    let limits = Limits::new(nums[0], nums.get(1).copied());
    if !limits.is_valid(hard_max) {
        return Err(
            items
                .first()
                .map(|s| s.err("limits out of range"))
                .unwrap_or_else(|| TextError::new(1, 1, "limits out of range")),
        );
    }
    Ok(limits)
}

fn parse_table_limits(items: &[Sexpr]) -> TextResult<(Limits, ValType)> {
    let mut nums = Vec::new();
    let mut elem = ValType::FuncRef;
    for s in items {
        if let Some(a) = s.atom() {
            if let Ok(t) = parse_ref_type(s) {
                elem = t;
                continue;
            }
            let v = parse_u32_str(a).ok_or_else(|| s.err(format!("invalid table limit {a:?}")))?;
            nums.push(v);
        } else {
            return Err(s.err("malformed table limits"));
        }
    }
    if nums.is_empty() || nums.len() > 2 {
        return Err(
            items
                .first()
                .map(|s| s.err("malformed table limits"))
                .unwrap_or_else(|| TextError::new(1, 1, "malformed table limits")),
        );
    }
    Ok((Limits::new(nums[0], nums.get(1).copied()), elem))
}

fn parse_global_type(s: &Sexpr) -> TextResult<GlobalType> {
    if let Some(inner) = s.list() {
        if inner.first().and_then(|x| x.atom()) == Some("mut") {
            let ty = parse_valtype(inner.get(1).ok_or_else(|| s.err("mut needs a type"))?)?;
            return Ok(GlobalType { val: ty, mutable: true });
        }
        return Err(s.err("malformed global type"));
    }
    Ok(GlobalType { val: parse_valtype(s)?, mutable: false })
}

// --- Instructions -----------------------------------------------------------

struct InstrCtx {
    type_names: HashMap<String, u32>,
    func_names: HashMap<String, u32>,
    table_names: HashMap<String, u32>,
    mem_names: HashMap<String, u32>,
    global_names: HashMap<String, u32>,
    type_base: u32,
    extra_types: Vec<FuncType>,
    locals: Vec<ValType>,
    local_names: HashMap<String, u32>,
    labels: Vec<Option<String>>,
}

impl InstrCtx {
    fn parse_seq(
        &mut self,
        toks: &[Sexpr],
        pos: &mut usize,
        stops: &[&str],
    ) -> TextResult<Vec<Instr>> {
        let mut out = Vec::new();
        while *pos < toks.len() {
            let tok = &toks[*pos];
            match &tok.kind {
                SexprKind::Atom(a) => {
                    if stops.contains(&a.as_str()) {
                        return Ok(out);
                    }
                    if a == "end" || a == "else" || a == "then" {
                        return Err(tok.err(format!("unexpected {a}")));
                    }
                    match a.as_str() {
                        "block" | "loop" | "if" => {
                            *pos += 1;
                            let label = self.take_label(toks, pos);
                            let bt = self.parse_block_type(toks, pos)?;
                            self.labels.push(label);
                            if a == "if" {
                                out.push(Instr::If(bt));
                                let then_body = self.parse_seq(toks, pos, &["else", "end"])?;
                                out.extend(then_body);
                                if toks.get(*pos).and_then(|s| s.atom()) == Some("else") {
                                    *pos += 1;
                                    out.push(Instr::Else);
                                    let else_body = self.parse_seq(toks, pos, &["end"])?;
                                    out.extend(else_body);
                                }
                                if toks.get(*pos).and_then(|s| s.atom()) != Some("end") {
                                    return Err(tok.err("unterminated if"));
                                }
                                *pos += 1;
                                out.push(Instr::End);
                            } else {
                                out.push(if a == "block" {
                                    Instr::Block(bt)
                                } else {
                                    Instr::Loop(bt)
                                });
                                let body = self.parse_seq(toks, pos, &["end"])?;
                                out.extend(body);
                                if toks.get(*pos).and_then(|s| s.atom()) != Some("end") {
                                    return Err(tok.err("unterminated block"));
                                }
                                *pos += 1;
                                out.push(Instr::End);
                            }
                            self.labels.pop();
                        }
                        _ => {
                            *pos += 1;
                            let instr = self.flat_op(a, toks, pos, tok)?;
                            out.push(instr);
                        }
                    }
                }
                SexprKind::List(_) => {
                    *pos += 1;
                    out.extend(self.parse_folded(tok)?);
                }
                SexprKind::Str(_) => {
                    return Err(tok.err("unexpected string in instruction sequence"))
                }
            }
        }
        Ok(out)
    }

    fn take_label(&self, toks: &[Sexpr], pos: &mut usize) -> Option<String> {
        if let Some(a) = toks.get(*pos).and_then(|s| s.atom()) {
            if a.starts_with('$') {
                *pos += 1;
                return Some(a.to_string());
            }
        }
        None
    }

    fn parse_block_type(&mut self, toks: &[Sexpr], pos: &mut usize) -> TextResult<BlockType> {
        let mut explicit = None;
        let mut params = Vec::new();
        let mut results = Vec::new();
        while let Some(inner) = toks.get(*pos).and_then(|s| s.list()) {
            match inner.first().and_then(|s| s.atom()) {
                Some("type") => {
                    let idx = inner.get(1).ok_or_else(|| toks[*pos].err("missing type"))?;
                    let a = idx.atom().ok_or_else(|| idx.err("expected type"))?;
                    explicit = Some(if a.starts_with('$') {
                        *self
                            .type_names
                            .get(a)
                            .ok_or_else(|| idx.err(format!("unknown type {a}")))?
                    } else {
                        parse_u32_str(a).ok_or_else(|| idx.err("invalid type index"))?
                    });
                    *pos += 1;
                }
                Some("param") => {
                    for s in &inner[1..] {
                        if s.atom().map(|a| a.starts_with('$')).unwrap_or(false) {
                            continue;
                        }
                        params.push(parse_valtype(s)?);
                    }
                    *pos += 1;
                }
                Some("result") => {
                    for s in &inner[1..] {
                        results.push(parse_valtype(s)?);
                    }
                    *pos += 1;
                }
                _ => break,
            }
        }
        if let Some(idx) = explicit {
            return Ok(BlockType::TypeIdx(idx));
        }
        if params.is_empty() && results.is_empty() {
            Ok(BlockType::Empty)
        } else if params.is_empty() && results.len() == 1 {
            Ok(BlockType::Value(results[0]))
        } else {
            let idx = self.type_base + self.extra_types.len() as u32;
            self.extra_types.push(FuncType { params, results });
            Ok(BlockType::TypeIdx(idx))
        }
    }

    fn parse_folded(&mut self, s: &Sexpr) -> TextResult<Vec<Instr>> {
        let list = match s.list() {
            Some(l) if !l.is_empty() => l,
            _ => return Err(s.err("empty folded expression")),
        };
        let head = list[0]
            .atom()
            .ok_or_else(|| list[0].err("expected an instruction"))?;
        match head {
            "block" | "loop" => {
                let mut pos = 1;
                let label = self.take_label(list, &mut pos);
                let bt = self.parse_block_type(list, &mut pos)?;
                self.labels.push(label);
                let body = self.parse_seq(list, &mut pos, &[])?;
                self.labels.pop();
                let mut out = vec![if head == "block" {
                    Instr::Block(bt)
                } else {
                    Instr::Loop(bt)
                }];
                out.extend(body);
                out.push(Instr::End);
                Ok(out)
            }
            "if" => {
                let mut pos = 1;
                let label = self.take_label(list, &mut pos);
                let bt = self.parse_block_type(list, &mut pos)?;
                // Condition expressions come before (then ...).
                let mut cond = Vec::new();
                while let Some(item) = list.get(pos) {
                    if item.head() == Some("then") {
                        break;
                    }
                    cond.extend(self.operand(item)?);
                    pos += 1;
                }
                let then_list = list
                    .get(pos)
                    .ok_or_else(|| s.err("if is missing (then ...)"))?;
                let then_inner = then_list.list().ok_or_else(|| then_list.err("malformed then"))?;
                pos += 1;
                self.labels.push(label);
                let mut out = cond;
                out.push(Instr::If(bt));
                let mut tpos = 1;
                out.extend(self.parse_seq(then_inner, &mut tpos, &[])?);
                if let Some(else_list) = list.get(pos) {
                    if else_list.head() == Some("else") {
                        let inner = else_list.list().unwrap();
                        out.push(Instr::Else);
                        let mut epos = 1;
                        out.extend(self.parse_seq(inner, &mut epos, &[])?);
                        pos += 1;
                    }
                }
                if pos != list.len() {
                    return Err(s.err("trailing tokens in if"));
                }
                out.push(Instr::End);
                self.labels.pop();
                Ok(out)
            }
            "then" | "else" => Err(s.err(format!("{head} outside of if"))),
            _ => {
                let mut pos = 1;
                let instr = self.flat_op(head, list, &mut pos, s)?;
                let mut out = Vec::new();
                while pos < list.len() {
                    out.extend(self.operand(&list[pos])?);
                    pos += 1;
                }
                out.push(instr);
                Ok(out)
            }
        }
    }

    fn operand(&mut self, s: &Sexpr) -> TextResult<Vec<Instr>> {
        match &s.kind {
            SexprKind::List(_) => self.parse_folded(s),
            SexprKind::Atom(a) => Err(s.err(format!("unexpected atom {a:?} as operand"))),
            SexprKind::Str(_) => Err(s.err("unexpected string as operand")),
        }
    }

    fn label_index(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a label"))?;
        if a.starts_with('$') {
            for (i, label) in self.labels.iter().enumerate().rev() {
                if label.as_deref() == Some(a) {
                    return Ok((self.labels.len() - 1 - i) as u32);
                }
            }
            Err(s.err(format!("unknown label {a}")))
        } else {
            parse_u32_str(a).ok_or_else(|| s.err("invalid label index"))
        }
    }

    fn local_index(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a local"))?;
        if a.starts_with('$') {
            self.local_names
                .get(a)
                .copied()
                .ok_or_else(|| s.err(format!("unknown local {a}")))
        } else {
            let idx = parse_u32_str(a).ok_or_else(|| s.err("invalid local index"))?;
            if idx as usize >= self.locals.len() {
                return Err(s.err(format!("local index {idx} out of range")));
            }
            Ok(idx)
        }
    }

    fn global_index(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a global"))?;
        if a.starts_with('$') {
            self.global_names
                .get(a)
                .copied()
                .ok_or_else(|| s.err(format!("unknown global {a}")))
        } else {
            parse_u32_str(a).ok_or_else(|| s.err("invalid global index"))
        }
    }

    fn table_index(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a table"))?;
        if a.starts_with('$') {
            self.table_names
                .get(a)
                .copied()
                .ok_or_else(|| s.err(format!("unknown table {a}")))
        } else {
            parse_u32_str(a).ok_or_else(|| s.err("invalid table index"))
        }
    }

    fn memory_index(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a memory"))?;
        if a.starts_with('$') {
            self.mem_names
                .get(a)
                .copied()
                .ok_or_else(|| s.err(format!("unknown memory {a}")))
        } else {
            parse_u32_str(a).ok_or_else(|| s.err("invalid memory index"))
        }
    }

    fn func_index(&self, s: &Sexpr) -> TextResult<u32> {
        let a = s.atom().ok_or_else(|| s.err("expected a function"))?;
        if a.starts_with('$') {
            self.func_names
                .get(a)
                .copied()
                .ok_or_else(|| s.err(format!("unknown function {a}")))
        } else {
            parse_u32_str(a).ok_or_else(|| s.err("invalid function index"))
        }
    }

    fn constant_u32_operand(&self, s: &Sexpr) -> TextResult<u32> {
        parse_u32(s)
    }

    fn flat_op(
        &mut self,
        name: &str,
        toks: &[Sexpr],
        pos: &mut usize,
        at: &Sexpr,
    ) -> TextResult<Instr> {
        let take = |pos: &mut usize| -> TextResult<Sexpr> {
            let s = toks
                .get(*pos)
                .ok_or_else(|| at.err(format!("{name} is missing an immediate")))?
                .clone();
            *pos += 1;
            Ok(s)
        };
        if let Some(instr) = simple_instr(name) {
            return Ok(instr);
        }
        Ok(match name {
            "block" | "loop" | "if" => {
                return Err(at.err("control instruction cannot appear here"))
            }
            "br" => {
                let s = take(pos)?;
                Instr::Br(self.label_index(&s)?)
            }
            "br_if" => {
                let s = take(pos)?;
                Instr::BrIf(self.label_index(&s)?)
            }
            "br_table" => {
                let mut targets = Vec::new();
                while let Some(s) = toks.get(*pos) {
                    let is_label = s
                        .atom()
                        .map(|a| a.starts_with('$') || parse_u32_str(a).is_some())
                        .unwrap_or(false);
                    if !is_label {
                        break;
                    }
                    targets.push(self.label_index(s)?);
                    *pos += 1;
                }
                if targets.is_empty() {
                    return Err(at.err("br_table needs at least a default label"));
                }
                let default = targets.pop().unwrap();
                Instr::BrTable(Box::new(BrTable { targets, default }))
            }
            "call" => {
                let s = take(pos)?;
                Instr::Call(self.func_index(&s)?)
            }
            "call_indirect" => {
                let mut table = 0u32;
                let mut ty = None;
                let mut params = Vec::new();
                let mut results = Vec::new();
                while let Some(inner) = toks.get(*pos).and_then(|s| s.list()) {
                    match inner.first().and_then(|s| s.atom()) {
                        Some("type") => {
                            let idx = inner.get(1).ok_or_else(|| at.err("missing type"))?;
                            let a = idx.atom().ok_or_else(|| idx.err("expected type"))?;
                            ty = Some(if a.starts_with('$') {
                                *self.type_names.get(a).ok_or_else(|| idx.err("unknown type"))?
                            } else {
                                parse_u32_str(a).ok_or_else(|| idx.err("invalid type"))?
                            });
                            *pos += 1;
                        }
                        Some("param") => {
                            for s in &inner[1..] {
                                params.push(parse_valtype(s)?);
                            }
                            *pos += 1;
                        }
                        Some("result") => {
                            for s in &inner[1..] {
                                results.push(parse_valtype(s)?);
                            }
                            *pos += 1;
                        }
                        _ => break,
                    }
                }
                if ty.is_none() && (!params.is_empty() || !results.is_empty()) {
                    let idx = self.type_base + self.extra_types.len() as u32;
                    self.extra_types.push(FuncType { params, results });
                    ty = Some(idx);
                }
                if let Some(s) = toks.get(*pos) {
                    if let Some(a) = s.atom() {
                        if a.starts_with('$') || parse_u32_str(a).is_some() {
                            table = self.table_index(s)?;
                            *pos += 1;
                        }
                    }
                }
                Instr::CallIndirect { type_idx: ty.unwrap_or(0), table_idx: table }
            }
            "local.get" | "local.set" | "local.tee" => {
                let s = take(pos)?;
                let idx = self.local_index(&s)?;
                match name {
                    "local.get" => Instr::LocalGet(idx),
                    "local.set" => Instr::LocalSet(idx),
                    _ => Instr::LocalTee(idx),
                }
            }
            "global.get" | "global.set" => {
                let s = take(pos)?;
                let idx = self.global_index(&s)?;
                if name == "global.get" {
                    Instr::GlobalGet(idx)
                } else {
                    Instr::GlobalSet(idx)
                }
            }
            "table.get" | "table.set" | "table.size" | "table.grow" | "table.fill" => {
                let mut idx = 0u32;
                if let Some(s) = toks.get(*pos) {
                    if s.atom().map(|a| a.starts_with('$') || parse_u32_str(a).is_some()).unwrap_or(false) {
                        idx = self.table_index(s)?;
                        *pos += 1;
                    }
                }
                match name {
                    "table.get" => Instr::TableGet(idx),
                    "table.set" => Instr::TableSet(idx),
                    "table.size" => Instr::TableSize(idx),
                    "table.grow" => Instr::TableGrow(idx),
                    _ => Instr::TableFill(idx),
                }
            }
            "table.copy" => {
                let dst = {
                    let s = take(pos)?;
                    self.table_index(&s)?
                };
                let src = {
                    let s = take(pos)?;
                    self.table_index(&s)?
                };
                Instr::TableCopy { dst, src }
            }
            "table.init" => {
                let first = {
                    let s = take(pos)?;
                    s
                };
                let (table, elem) = if let Some(s) = toks.get(*pos) {
                    if s.atom().map(|a| a.starts_with('$') || parse_u32_str(a).is_some()).unwrap_or(false) {
                        let second = take(pos)?;
                        (self.table_index(&first)?, parse_u32(&second)?)
                    } else {
                        (0, parse_u32(&first)?)
                    }
                } else {
                    (0, parse_u32(&first)?)
                };
                Instr::TableInit { elem, table }
            }
            "elem.drop" => {
                let s = take(pos)?;
                Instr::ElemDrop(self.constant_u32_operand(&s)?)
            }
            "memory.size" | "memory.grow" => {
                let mut idx = 0u32;
                if let Some(s) = toks.get(*pos) {
                    if s.atom().map(|a| a.starts_with('$') || parse_u32_str(a).is_some()).unwrap_or(false) {
                        idx = self.memory_index(s)?;
                        *pos += 1;
                    }
                }
                if name == "memory.size" {
                    Instr::MemorySize(idx)
                } else {
                    Instr::MemoryGrow(idx)
                }
            }
            "memory.init" => {
                let first = take(pos)?;
                let data = parse_u32(&first)?;
                let mut mem = 0u32;
                if let Some(s) = toks.get(*pos) {
                    if s.atom().map(|a| a.starts_with('$') || parse_u32_str(a).is_some()).unwrap_or(false) {
                        mem = self.memory_index(s)?;
                        *pos += 1;
                    }
                }
                Instr::MemoryInit { data, mem }
            }
            "data.drop" => {
                let s = take(pos)?;
                Instr::DataDrop(parse_u32(&s)?)
            }
            "memory.copy" => {
                let mut dst = 0u32;
                let mut src = 0u32;
                if let Some(s) = toks.get(*pos) {
                    if s.atom().map(|a| a.starts_with('$') || parse_u32_str(a).is_some()).unwrap_or(false) {
                        dst = self.memory_index(s)?;
                        *pos += 1;
                        let s2 = take(pos)?;
                        src = self.memory_index(&s2)?;
                    }
                }
                Instr::MemoryCopy { dst, src }
            }
            "memory.fill" => {
                let mut mem = 0u32;
                if let Some(s) = toks.get(*pos) {
                    if s.atom().map(|a| a.starts_with('$') || parse_u32_str(a).is_some()).unwrap_or(false) {
                        mem = self.memory_index(s)?;
                        *pos += 1;
                    }
                }
                Instr::MemoryFill(mem)
            }
            "i32.const" => Instr::I32Const(parse_i32(&take(pos)?)?),
            "i64.const" => Instr::I64Const(parse_i64(&take(pos)?)?),
            "f32.const" => Instr::F32Const(parse_f32(&take(pos)?)?),
            "f64.const" => Instr::F64Const(parse_f64(&take(pos)?)?),
            "ref.null" => {
                let s = take(pos)?;
                Instr::RefNull(parse_ref_type(&s)?)
            }
            "ref.func" => {
                let s = take(pos)?;
                Instr::RefFunc(self.func_index(&s)?)
            }
            "select" => {
                if let Some(inner) = toks.get(*pos).and_then(|s| s.list()) {
                    if inner.first().and_then(|s| s.atom()) == Some("result") {
                        let mut tys = Vec::new();
                        for s in &inner[1..] {
                            tys.push(parse_valtype(s)?);
                        }
                        *pos += 1;
                        Instr::SelectT(tys)
                    } else {
                        Instr::Select
                    }
                } else {
                    Instr::Select
                }
            }
            _ => {
                if let Some(kind) = load_kind(name) {
                    let (arg, mem) = self.memarg(toks, pos, kind.width(), at)?;
                    Instr::Load { kind, mem, arg }
                } else if let Some(kind) = store_kind(name) {
                    let (arg, mem) = self.memarg(toks, pos, kind.width(), at)?;
                    Instr::Store { kind, mem, arg }
                } else {
                    return Err(at.err(format!("unknown instruction {name:?}")));
                }
            }
        })
    }

    fn memarg(
        &self,
        toks: &[Sexpr],
        pos: &mut usize,
        width: u32,
        at: &Sexpr,
    ) -> TextResult<(MemArg, u32)> {
        let mut offset = 0u32;
        let mut align = None;
        let mut mem = 0u32;
        loop {
            let Some(s) = toks.get(*pos) else { break };
            let Some(a) = s.atom() else { break };
            if let Some(rest) = a.strip_prefix("offset=") {
                offset = parse_u32_str(rest).ok_or_else(|| s.err("invalid offset"))?;
                *pos += 1;
            } else if let Some(rest) = a.strip_prefix("align=") {
                let bytes = parse_u32_str(rest).ok_or_else(|| s.err("invalid alignment"))?;
                if bytes == 0 || !bytes.is_power_of_two() {
                    return Err(s.err("alignment must be a power of two"));
                }
                align = Some(bytes.trailing_zeros());
                *pos += 1;
            } else if a.starts_with('$') || parse_u32_str(a).is_some() {
                mem = self.memory_index(s)?;
                *pos += 1;
                break;
            } else {
                break;
            }
        }
        let _ = at;
        Ok((
            MemArg { align: align.unwrap_or(width.trailing_zeros()), offset },
            mem,
        ))
    }
}

fn simple_instr(name: &str) -> Option<Instr> {
    use Instr::*;
    Some(match name {
        "unreachable" => Unreachable,
        "nop" => Nop,
        "return" => Return,
        "drop" => Drop,
        "i32.eqz" => I32Eqz,
        "i32.eq" => I32Eq,
        "i32.ne" => I32Ne,
        "i32.lt_s" => I32LtS,
        "i32.lt_u" => I32LtU,
        "i32.gt_s" => I32GtS,
        "i32.gt_u" => I32GtU,
        "i32.le_s" => I32LeS,
        "i32.le_u" => I32LeU,
        "i32.ge_s" => I32GeS,
        "i32.ge_u" => I32GeU,
        "i64.eqz" => I64Eqz,
        "i64.eq" => I64Eq,
        "i64.ne" => I64Ne,
        "i64.lt_s" => I64LtS,
        "i64.lt_u" => I64LtU,
        "i64.gt_s" => I64GtS,
        "i64.gt_u" => I64GtU,
        "i64.le_s" => I64LeS,
        "i64.le_u" => I64LeU,
        "i64.ge_s" => I64GeS,
        "i64.ge_u" => I64GeU,
        "f32.eq" => F32Eq,
        "f32.ne" => F32Ne,
        "f32.lt" => F32Lt,
        "f32.gt" => F32Gt,
        "f32.le" => F32Le,
        "f32.ge" => F32Ge,
        "f64.eq" => F64Eq,
        "f64.ne" => F64Ne,
        "f64.lt" => F64Lt,
        "f64.gt" => F64Gt,
        "f64.le" => F64Le,
        "f64.ge" => F64Ge,
        "i32.clz" => I32Clz,
        "i32.ctz" => I32Ctz,
        "i32.popcnt" => I32Popcnt,
        "i32.add" => I32Add,
        "i32.sub" => I32Sub,
        "i32.mul" => I32Mul,
        "i32.div_s" => I32DivS,
        "i32.div_u" => I32DivU,
        "i32.rem_s" => I32RemS,
        "i32.rem_u" => I32RemU,
        "i32.and" => I32And,
        "i32.or" => I32Or,
        "i32.xor" => I32Xor,
        "i32.shl" => I32Shl,
        "i32.shr_s" => I32ShrS,
        "i32.shr_u" => I32ShrU,
        "i32.rotl" => I32Rotl,
        "i32.rotr" => I32Rotr,
        "i64.clz" => I64Clz,
        "i64.ctz" => I64Ctz,
        "i64.popcnt" => I64Popcnt,
        "i64.add" => I64Add,
        "i64.sub" => I64Sub,
        "i64.mul" => I64Mul,
        "i64.div_s" => I64DivS,
        "i64.div_u" => I64DivU,
        "i64.rem_s" => I64RemS,
        "i64.rem_u" => I64RemU,
        "i64.and" => I64And,
        "i64.or" => I64Or,
        "i64.xor" => I64Xor,
        "i64.shl" => I64Shl,
        "i64.shr_s" => I64ShrS,
        "i64.shr_u" => I64ShrU,
        "i64.rotl" => I64Rotl,
        "i64.rotr" => I64Rotr,
        "f32.abs" => F32Abs,
        "f32.neg" => F32Neg,
        "f32.ceil" => F32Ceil,
        "f32.floor" => F32Floor,
        "f32.trunc" => F32Trunc,
        "f32.nearest" => F32Nearest,
        "f32.sqrt" => F32Sqrt,
        "f32.add" => F32Add,
        "f32.sub" => F32Sub,
        "f32.mul" => F32Mul,
        "f32.div" => F32Div,
        "f32.min" => F32Min,
        "f32.max" => F32Max,
        "f32.copysign" => F32Copysign,
        "f64.abs" => F64Abs,
        "f64.neg" => F64Neg,
        "f64.ceil" => F64Ceil,
        "f64.floor" => F64Floor,
        "f64.trunc" => F64Trunc,
        "f64.nearest" => F64Nearest,
        "f64.sqrt" => F64Sqrt,
        "f64.add" => F64Add,
        "f64.sub" => F64Sub,
        "f64.mul" => F64Mul,
        "f64.div" => F64Div,
        "f64.min" => F64Min,
        "f64.max" => F64Max,
        "f64.copysign" => F64Copysign,
        "i32.wrap_i64" => I32WrapI64,
        "i32.trunc_f32_s" => I32TruncF32S,
        "i32.trunc_f32_u" => I32TruncF32U,
        "i32.trunc_f64_s" => I32TruncF64S,
        "i32.trunc_f64_u" => I32TruncF64U,
        "i64.extend_i32_s" => I64ExtendI32S,
        "i64.extend_i32_u" => I64ExtendI32U,
        "i64.trunc_f32_s" => I64TruncF32S,
        "i64.trunc_f32_u" => I64TruncF32U,
        "i64.trunc_f64_s" => I64TruncF64S,
        "i64.trunc_f64_u" => I64TruncF64U,
        "f32.convert_i32_s" => F32ConvertI32S,
        "f32.convert_i32_u" => F32ConvertI32U,
        "f32.convert_i64_s" => F32ConvertI64S,
        "f32.convert_i64_u" => F32ConvertI64U,
        "f32.demote_f64" => F32DemoteF64,
        "f64.convert_i32_s" => F64ConvertI32S,
        "f64.convert_i32_u" => F64ConvertI32U,
        "f64.convert_i64_s" => F64ConvertI64S,
        "f64.convert_i64_u" => F64ConvertI64U,
        "f64.promote_f32" => F64PromoteF32,
        "i32.reinterpret_f32" => I32ReinterpretF32,
        "i64.reinterpret_f64" => I64ReinterpretF64,
        "f32.reinterpret_i32" => F32ReinterpretI32,
        "f64.reinterpret_i64" => F64ReinterpretI64,
        "i32.extend8_s" => I32Extend8S,
        "i32.extend16_s" => I32Extend16S,
        "i64.extend8_s" => I64Extend8S,
        "i64.extend16_s" => I64Extend16S,
        "i64.extend32_s" => I64Extend32S,
        "i32.trunc_sat_f32_s" => I32TruncSatF32S,
        "i32.trunc_sat_f32_u" => I32TruncSatF32U,
        "i32.trunc_sat_f64_s" => I32TruncSatF64S,
        "i32.trunc_sat_f64_u" => I32TruncSatF64U,
        "i64.trunc_sat_f32_s" => I64TruncSatF32S,
        "i64.trunc_sat_f32_u" => I64TruncSatF32U,
        "i64.trunc_sat_f64_s" => I64TruncSatF64S,
        "i64.trunc_sat_f64_u" => I64TruncSatF64U,
        "ref.is_null" => RefIsNull,
        _ => return None,
    })
}

fn load_kind(name: &str) -> Option<LoadKind> {
    Some(match name {
        "i32.load" => LoadKind::I32,
        "i64.load" => LoadKind::I64,
        "f32.load" => LoadKind::F32,
        "f64.load" => LoadKind::F64,
        "i32.load8_s" => LoadKind::I32x8S,
        "i32.load8_u" => LoadKind::I32x8U,
        "i32.load16_s" => LoadKind::I32x16S,
        "i32.load16_u" => LoadKind::I32x16U,
        "i64.load8_s" => LoadKind::I64x8S,
        "i64.load8_u" => LoadKind::I64x8U,
        "i64.load16_s" => LoadKind::I64x16S,
        "i64.load16_u" => LoadKind::I64x16U,
        "i64.load32_s" => LoadKind::I64x32S,
        "i64.load32_u" => LoadKind::I64x32U,
        _ => return None,
    })
}

fn store_kind(name: &str) -> Option<StoreKind> {
    Some(match name {
        "i32.store" => StoreKind::I32,
        "i64.store" => StoreKind::I64,
        "f32.store" => StoreKind::F32,
        "f64.store" => StoreKind::F64,
        "i32.store8" => StoreKind::I32x8,
        "i32.store16" => StoreKind::I32x16,
        "i64.store8" => StoreKind::I64x8,
        "i64.store16" => StoreKind::I64x16,
        "i64.store32" => StoreKind::I64x32,
        _ => return None,
    })
}
