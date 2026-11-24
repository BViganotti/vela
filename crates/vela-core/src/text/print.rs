//! Render a decoded module back to WAT text. Used for examples, diagnostics
//! and the Studio's text view of binary modules.

use crate::instr::Instr;
use crate::module::{
    DataMode, Element, ElementMode, ExportDesc, Func, FuncBody, ImportDesc, Module,
};
use crate::types::{FuncType, Limits, ValType};

pub fn module_to_wat(module: &Module) -> String {
    let mut out = String::new();
    out.push_str("(module\n");
    for (i, ty) in module.types.iter().enumerate() {
        out.push_str(&format!("  (type (;{i};) {})\n", func_type(ty)));
    }
    for imp in &module.imports {
        let desc = match imp.desc {
            ImportDesc::Func(t) => format!("(func (type {t}))"),
            ImportDesc::Table(t) => {
                format!("(table {} {})", limits(t.limits), t.elem.name())
            }
            ImportDesc::Memory(m) => format!("(memory {})", limits(m.limits)),
            ImportDesc::Global(g) => format!("(global {})", global_type(g)),
        };
        out.push_str(&format!(
            "  (import {} {} {desc})\n",
            quote_str(imp.module.as_bytes()),
            quote_str(imp.name.as_bytes())
        ));
    }
    for (i, table) in module.tables.iter().enumerate() {
        out.push_str(&format!(
            "  (table (;{i};) {} {})\n",
            limits(table.limits),
            table.elem.name()
        ));
    }
    for (i, mem) in module.memories.iter().enumerate() {
        out.push_str(&format!("  (memory (;{i};) {})\n", limits(mem.limits)));
    }
    for (i, g) in module.globals.iter().enumerate() {
        out.push_str(&format!(
            "  (global (;{i};) {} {})\n",
            global_type(g.ty),
            inline_expr(&g.init)
        ));
    }
    let imported_funcs = module.imported_funcs();
    for (i, f) in module.funcs.iter().enumerate() {
        func_to_wat(&mut out, f, imported_funcs + i, &module.types);
    }
    for e in &module.exports {
        let (kind, idx) = match e.desc {
            ExportDesc::Func(i) => ("func", i),
            ExportDesc::Table(i) => ("table", i),
            ExportDesc::Memory(i) => ("memory", i),
            ExportDesc::Global(i) => ("global", i),
        };
        out.push_str(&format!(
            "  (export {} ({kind} {idx}))\n",
            quote_str(e.name.as_bytes())
        ));
    }
    if let Some(start) = module.start {
        out.push_str(&format!("  (start {start})\n"));
    }
    for e in &module.elems {
        elem_to_wat(&mut out, e);
    }
    for d in &module.datas {
        match &d.mode {
            DataMode::Passive => {
                out.push_str(&format!("  (data {})\n", quote_bytes(&d.bytes)));
            }
            DataMode::Active { memory, offset } => {
                out.push_str(&format!(
                    "  (data (memory {memory}) (offset {}) {})\n",
                    inline_expr(offset),
                    quote_bytes(&d.bytes)
                ));
            }
        }
    }
    out.push_str(")\n");
    out
}

pub fn func_type(ty: &FuncType) -> String {
    let mut s = String::from("(func");
    if !ty.params.is_empty() {
        s.push_str(" (param");
        for p in &ty.params {
            s.push(' ');
            s.push_str(p.name());
        }
        s.push(')');
    }
    if !ty.results.is_empty() {
        s.push_str(" (result");
        for r in &ty.results {
            s.push(' ');
            s.push_str(r.name());
        }
        s.push(')');
    }
    s.push(')');
    s
}

fn limits(l: Limits) -> String {
    match l.max {
        Some(max) => format!("{} {}", l.min, max),
        None => format!("{}", l.min),
    }
}

fn global_type(g: crate::types::GlobalType) -> String {
    if g.mutable {
        format!("(mut {})", g.val.name())
    } else {
        g.val.name().to_string()
    }
}

fn func_to_wat(out: &mut String, f: &Func, index: usize, types: &[FuncType]) {
    let ty = &types[f.type_idx as usize];
    out.push_str(&format!("  (func (;{index};) (type {})", f.type_idx));
    if !f.body.locals.is_empty() {
        out.push_str(" (local");
        for l in &f.body.locals {
            out.push(' ');
            out.push_str(l.name());
        }
        out.push(')');
    }
    out.push('\n');
    body_to_wat(out, &f.body);
    out.push_str("  )\n");
    let _ = ty;
}

fn body_to_wat(out: &mut String, body: &FuncBody) {
    let mut indent = 2usize;
    // The function's final `end` is implicit in the text format.
    let instrs: &[Instr] = match body.body.last() {
        Some(Instr::End) => &body.body[..body.body.len() - 1],
        _ => &body.body,
    };
    for instr in instrs {
        match instr {
            Instr::End => {
                indent = indent.saturating_sub(1);
            }
            Instr::Else => {
                indent = indent.saturating_sub(1);
            }
            _ => {}
        }
        out.push_str(&"  ".repeat(indent));
        out.push_str(&instr_text(instr));
        out.push('\n');
        match instr {
            Instr::Block(_) | Instr::Loop(_) | Instr::If(_) | Instr::Else => {
                indent += 1;
            }
            _ => {}
        }
    }
}

fn elem_to_wat(out: &mut String, e: &Element) {
    let (prefix, use_funcs) = match &e.mode {
        ElementMode::Active { table, offset } => (
            format!("(table {table}) (offset {})", inline_expr(offset)),
            true,
        ),
        ElementMode::Passive => ("".to_string(), false),
        ElementMode::Declarative => ("declare".to_string(), false),
    };
    let all_funcs = e
        .init
        .iter()
        .all(|item| matches!(item.as_slice(), [Instr::RefFunc(_), Instr::End]));
    out.push_str("  (elem");
    if !prefix.is_empty() {
        out.push(' ');
        out.push_str(&prefix);
    }
    if all_funcs && e.elem_type == ValType::FuncRef {
        if use_funcs {
            out.push_str(" func");
        } else {
            out.push_str(" func");
        }
        for item in &e.init {
            if let Instr::RefFunc(i) = item[0] {
                out.push_str(&format!(" {i}"));
            }
        }
    } else {
        out.push(' ');
        out.push_str(e.elem_type.name());
        for item in &e.init {
            out.push(' ');
            out.push('(');
            out.push_str(&inline_expr(item));
            out.push(')');
        }
    }
    out.push_str(")\n");
}

pub fn inline_expr(expr: &[Instr]) -> String {
    let real: Vec<&Instr> = expr.iter().filter(|i| !matches!(i, Instr::End)).collect();
    real.iter()
        .map(|i| instr_text(i))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn instr_text(instr: &Instr) -> String {
    crate::instr::disassemble(instr)
}

fn quote_bytes(bytes: &[u8]) -> String {
    let mut s = String::from("\"");
    for b in bytes {
        match b {
            b'\n' => s.push_str("\\n"),
            b'\t' => s.push_str("\\t"),
            b'\r' => s.push_str("\\r"),
            b'"' => s.push_str("\\\""),
            b'\\' => s.push_str("\\\\"),
            0x20..=0x7e => s.push(*b as char),
            other => s.push_str(&format!("\\{other:02x}")),
        }
    }
    s.push('"');
    s
}

fn quote_str(bytes: &[u8]) -> String {
    quote_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_a_simple_module() {
        let m = crate::text::parse_wat(
            r#"(module
                 (type (func (param i32) (result i32)))
                 (memory 1 2)
                 (global (mut i32) (i32.const 7))
                 (func (type 0) (param i32) (result i32)
                   local.get 0
                   i32.const 1
                   i32.add)
                 (export "inc" (func 0))
                 (data (i32.const 8) "hi"))"#,
        )
        .unwrap();
        let text = module_to_wat(&m);
        assert!(text.contains("(type (;0;) (func (param i32) (result i32)))"));
        assert!(text.contains("local.get 0"));
        assert!(text.contains("(data (memory 0) (offset i32.const 8) \"hi\")"));
        let m2 = crate::text::parse_wat(&text).unwrap();
        assert_eq!(m.types, m2.types);
        assert_eq!(m.funcs, m2.funcs);
        assert_eq!(m.memories, m2.memories);
        assert_eq!(m.globals, m2.globals);
        assert_eq!(m.datas, m2.datas);
    }

    #[test]
    fn block_indentation_round_trips() {
        let m = crate::text::parse_wat(
            r#"(module (func (result i32)
                 (block (result i32) (i32.const 1))))"#,
        )
        .unwrap();
        let text = module_to_wat(&m);
        assert!(text.contains("    block"));
        assert!(text.contains("      i32.const 1"));
        let m2 = crate::text::parse_wat(&text).unwrap();
        assert_eq!(m.funcs, m2.funcs);
    }
}
