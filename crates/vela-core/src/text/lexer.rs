//! Lexer for WebAssembly text format and `.wast` scripts.
//!
//! The lexer is byte-oriented where it matters (string literals are byte
//! strings) and tracks line/column for diagnostics. Block comments nest.

use crate::error::{TextError, TextResult};

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    LParen,
    RParen,
    Atom(String),
    Str(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: u32,
    pub col: u32,
}

/// Maximum accepted text input size (16 MiB).
pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    line: u32,
    col: u32,
}

pub fn tokenize(input: &str) -> TextResult<Vec<Token>> {
    if input.len() > MAX_TEXT_BYTES {
        return Err(TextError::new(1, 1, "text input too large"));
    }
    let mut lx = Lexer {
        chars: input.chars().peekable(),
        line: 1,
        col: 1,
    };
    let mut tokens = Vec::new();
    loop {
        lx.skip_trivia()?;
        let line = lx.line;
        let col = lx.col;
        let c = match lx.peek() {
            Some(c) => c,
            None => break,
        };
        let kind = match c {
            '(' => {
                lx.bump();
                TokenKind::LParen
            }
            ')' => {
                lx.bump();
                TokenKind::RParen
            }
            '"' => TokenKind::Str(lx.string()?),
            _ => {
                let atom = lx.atom()?;
                TokenKind::Atom(atom)
            }
        };
        tokens.push(Token { kind, line, col });
        if tokens.len() > 8_000_000 {
            return Err(TextError::new(line, col, "too many tokens"));
        }
    }
    Ok(tokens)
}

impl<'a> Lexer<'a> {
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip_trivia(&mut self) -> TextResult<()> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some(';') => {
                    // Line comment: ;; ...
                    let mut clone = self.chars.clone();
                    clone.next();
                    if clone.next() == Some(';') {
                        self.bump();
                        self.bump();
                        while let Some(c) = self.peek() {
                            self.bump();
                            if c == '\n' {
                                break;
                            }
                        }
                    } else {
                        // A single ';' is not a valid token start; handled by atom().
                        return Ok(());
                    }
                }
                Some('(') => {
                    let mut clone = self.chars.clone();
                    clone.next();
                    if clone.next() == Some(';') {
                        self.bump();
                        self.bump();
                        self.block_comment()?;
                    } else {
                        return Ok(());
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn block_comment(&mut self) -> TextResult<()> {
        let mut depth = 1usize;
        while let Some(c) = self.bump() {
            if c == '(' {
                if self.peek() == Some(';') {
                    self.bump();
                    depth += 1;
                    if depth > 1000 {
                        return Err(TextError::new(
                            self.line,
                            self.col,
                            "comment nesting too deep",
                        ));
                    }
                }
            } else if c == ';' && self.peek() == Some(')') {
                self.bump();
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            }
        }
        Err(TextError::new(
            self.line,
            self.col,
            "unterminated block comment",
        ))
    }

    fn atom(&mut self) -> TextResult<String> {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == '(' || c == ')' || c == '"' || c == ';' {
                break;
            }
            out.push(c);
            self.bump();
        }
        if out.is_empty() {
            return Err(TextError::new(self.line, self.col, "unexpected character"));
        }
        Ok(out)
    }

    fn string(&mut self) -> TextResult<Vec<u8>> {
        let start_line = self.line;
        let start_col = self.col;
        self.bump(); // opening quote
        let mut out = Vec::new();
        loop {
            let c = match self.bump() {
                Some(c) => c,
                None => return Err(TextError::new(start_line, start_col, "unterminated string")),
            };
            match c {
                '"' => return Ok(out),
                '\\' => self.escape(&mut out)?,
                '\n' | '\r' => {
                    return Err(TextError::new(
                        self.line,
                        self.col,
                        "newline in string literal",
                    ))
                }
                c if (c as u32) < 0x20 || c == '\u{7f}' => {
                    return Err(TextError::new(
                        self.line,
                        self.col,
                        "control character in string literal",
                    ))
                }
                c => {
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
            if out.len() > 64 * 1024 * 1024 {
                return Err(TextError::new(
                    self.line,
                    self.col,
                    "string literal too long",
                ));
            }
        }
    }

    fn escape(&mut self, out: &mut Vec<u8>) -> TextResult<()> {
        let c = match self.bump() {
            Some(c) => c,
            None => return Err(TextError::new(self.line, self.col, "unterminated escape")),
        };
        match c {
            'n' => out.push(b'\n'),
            't' => out.push(b'\t'),
            'r' => out.push(b'\r'),
            '\\' => out.push(b'\\'),
            '"' => out.push(b'"'),
            '\'' => out.push(b'\''),
            'u' => {
                if self.bump() != Some('{') {
                    return Err(TextError::new(
                        self.line,
                        self.col,
                        "malformed unicode escape",
                    ));
                }
                let mut value: u32 = 0;
                let mut digits = 0;
                loop {
                    let d = match self.bump() {
                        Some(d) => d,
                        None => {
                            return Err(TextError::new(
                                self.line,
                                self.col,
                                "unterminated unicode escape",
                            ))
                        }
                    };
                    if d == '}' {
                        break;
                    }
                    let digit = d
                        .to_digit(16)
                        .ok_or_else(|| TextError::new(self.line, self.col, "bad hex digit"))?;
                    value = value.saturating_mul(16).saturating_add(digit);
                    digits += 1;
                    if digits > 6 {
                        return Err(TextError::new(
                            self.line,
                            self.col,
                            "unicode escape too long",
                        ));
                    }
                }
                if digits == 0 {
                    return Err(TextError::new(self.line, self.col, "empty unicode escape"));
                }
                let ch = char::from_u32(value)
                    .ok_or_else(|| TextError::new(self.line, self.col, "invalid unicode scalar"))?;
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            other => {
                let hi = other
                    .to_digit(16)
                    .ok_or_else(|| TextError::new(self.line, self.col, "bad escape"))?;
                let second = self
                    .bump()
                    .and_then(|c| c.to_digit(16))
                    .ok_or_else(|| TextError::new(self.line, self.col, "bad hex escape"))?;
                out.push(((hi * 16 + second) & 0xff) as u8);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atoms(src: &str) -> Vec<String> {
        tokenize(src)
            .unwrap()
            .into_iter()
            .filter_map(|t| match t.kind {
                TokenKind::Atom(a) => Some(a),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn atoms_and_parens() {
        let toks = tokenize("(module (func $f))").unwrap();
        assert_eq!(toks.len(), 7);
        assert!(matches!(toks[0].kind, TokenKind::LParen));
        assert_eq!(atoms("module $f 42"), vec!["module", "$f", "42"]);
    }

    #[test]
    fn comments() {
        assert_eq!(atoms("a ;; comment\nb"), vec!["a", "b"]);
        assert_eq!(atoms("a (; nested (; c ;) ;) b"), vec!["a", "b"]);
        assert!(tokenize("(; unterminated").is_err());
    }

    #[test]
    fn string_escapes() {
        let toks = tokenize(r#""a\n\41\u{1F600}""#).unwrap();
        match &toks[0].kind {
            TokenKind::Str(bytes) => {
                assert_eq!(&bytes[..3], b"a\nA");
                assert_eq!(std::str::from_utf8(&bytes[3..]).unwrap(), "\u{1F600}");
            }
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn bad_string() {
        assert!(tokenize("\"abc").is_err());
        assert!(tokenize("\"a\nb\"").is_err());
        assert!(tokenize("\"\\q\"").is_err());
        assert!(tokenize("\"\\u{d800}\"").is_err());
    }

    #[test]
    fn positions() {
        let toks = tokenize("\n  (x)").unwrap();
        assert_eq!(toks[0].line, 2);
        assert_eq!(toks[0].col, 3);
    }
}
