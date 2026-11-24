//! S-expression tree built from tokens, with bounded nesting.

use crate::error::{TextError, TextResult};
use crate::text::lexer::{Token, TokenKind};

pub const MAX_DEPTH: usize = 1_000;

#[derive(Clone, Debug, PartialEq)]
pub enum SexprKind {
    Atom(String),
    Str(Vec<u8>),
    List(Vec<Sexpr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sexpr {
    pub kind: SexprKind,
    pub line: u32,
    pub col: u32,
}

impl Sexpr {
    pub fn atom(&self) -> Option<&str> {
        match &self.kind {
            SexprKind::Atom(a) => Some(a),
            _ => None,
        }
    }

    pub fn list(&self) -> Option<&[Sexpr]> {
        match &self.kind {
            SexprKind::List(v) => Some(v),
            _ => None,
        }
    }

    pub fn bytes(&self) -> Option<&[u8]> {
        match &self.kind {
            SexprKind::Str(b) => Some(b),
            _ => None,
        }
    }

    pub fn is_list(&self) -> bool {
        matches!(self.kind, SexprKind::List(_))
    }

    pub fn head(&self) -> Option<&str> {
        self.list()?.first()?.atom()
    }

    pub fn err(&self, msg: impl Into<String>) -> TextError {
        TextError::new(self.line, self.col, msg)
    }

    /// Render back to canonical text, used by diagnostics and tests.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        self.write_text(&mut out);
        out
    }

    fn write_text(&self, out: &mut String) {
        match &self.kind {
            SexprKind::Atom(a) => out.push_str(a),
            SexprKind::Str(bytes) => {
                out.push('"');
                for b in bytes {
                    match b {
                        b'\n' => out.push_str("\\n"),
                        b'\t' => out.push_str("\\t"),
                        b'\r' => out.push_str("\\r"),
                        b'"' => out.push_str("\\\""),
                        b'\\' => out.push_str("\\\\"),
                        0x20..=0x7e => out.push(*b as char),
                        other => out.push_str(&format!("\\{other:02x}")),
                    }
                }
                out.push('"');
            }
            SexprKind::List(items) => {
                out.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    item.write_text(out);
                }
                out.push(')');
            }
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

pub fn parse(tokens: Vec<Token>) -> TextResult<Vec<Sexpr>> {
    let mut p = Parser { tokens, pos: 0 };
    let mut out = Vec::new();
    while p.pos < p.tokens.len() {
        out.push(p.parse_one(0)?);
    }
    Ok(out)
}

pub fn parse_str(input: &str) -> TextResult<Vec<Sexpr>> {
    parse(crate::text::lexer::tokenize(input)?)
}

impl Parser {
    fn parse_one(&mut self, depth: usize) -> TextResult<Sexpr> {
        if depth > MAX_DEPTH {
            return Err(TextError::new(1, 1, "s-expression nesting too deep"));
        }
        let tok = self.tokens[self.pos].clone();
        self.pos += 1;
        match tok.kind {
            TokenKind::Atom(a) => Ok(Sexpr {
                kind: SexprKind::Atom(a),
                line: tok.line,
                col: tok.col,
            }),
            TokenKind::Str(b) => Ok(Sexpr {
                kind: SexprKind::Str(b),
                line: tok.line,
                col: tok.col,
            }),
            TokenKind::RParen => Err(TextError::new(tok.line, tok.col, "unexpected ')'")),
            TokenKind::LParen => {
                let mut items = Vec::new();
                loop {
                    if self.pos >= self.tokens.len() {
                        return Err(TextError::new(tok.line, tok.col, "unclosed '('"));
                    }
                    if matches!(self.tokens[self.pos].kind, TokenKind::RParen) {
                        self.pos += 1;
                        break;
                    }
                    items.push(self.parse_one(depth + 1)?);
                }
                Ok(Sexpr {
                    kind: SexprKind::List(items),
                    line: tok.line,
                    col: tok.col,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let src = r#"(module (func $f (param i32) (result i32) local.get 0))"#;
        let parsed = parse_str(src).unwrap();
        assert_eq!(parsed[0].to_text(), src);
    }

    #[test]
    fn strings_round_trip() {
        let parsed = parse_str(r#"(data (i32.const 0) "a\00b\n")"#).unwrap();
        let data = parsed[0].list().unwrap();
        assert!(matches!(data[2].kind, SexprKind::Str(_)));
    }

    #[test]
    fn rejects_imbalance() {
        assert!(parse_str("(a (b)").is_err());
        assert!(parse_str("(a))").is_err());
        assert!(parse_str(")").is_err());
    }

    #[test]
    fn depth_bound() {
        let src = "(".repeat(MAX_DEPTH + 10);
        assert!(parse_str(&src).is_err());
    }

    #[test]
    fn nested_lists() {
        let parsed = parse_str("(a (b (c)))").unwrap();
        let l = parsed[0].list().unwrap();
        assert_eq!(l[1].head(), Some("b"));
    }
}
