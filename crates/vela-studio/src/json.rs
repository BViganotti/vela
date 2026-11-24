//! A small JSON writer/parser. No dependencies, bounded nesting.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn obj(pairs: Vec<(&str, Json)>) -> Json {
        Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    pub fn arr(items: Vec<Json>) -> Json {
        Json::Arr(items)
    }

    pub fn str(s: impl Into<String>) -> Json {
        Json::Str(s.into())
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Int(v) => Some(*v),
            Json::Float(v) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(v) => Some(v),
            _ => None,
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Int(v) => out.push_str(&v.to_string()),
            Json::Float(v) => {
                if v.is_finite() {
                    out.push_str(&format!("{v}"));
                } else {
                    out.push_str("null");
                }
            }
            Json::Str(s) => write_string(out, s),
            Json::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Json::Obj(pairs) => {
                out.push('{');
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_string(out, k);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

/// Parse a JSON document. Limits: 2 MiB input and 128 levels of nesting.
pub fn parse(input: &str) -> Result<Json, String> {
    if input.len() > 2 * 1024 * 1024 {
        return Err("json input too large".to_string());
    }
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
        depth: 0,
    };
    p.skip_ws();
    let v = p.value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(format!("trailing data at byte {}", p.pos));
    }
    Ok(v)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while let Some(b) = self.bytes.get(self.pos) {
            if matches!(b, b' ' | b'\n' | b'\r' | b'\t') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at byte {}", b as char, self.pos))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        if self.depth > 128 {
            return Err("json nesting too deep".to_string());
        }
        self.skip_ws();
        let b = self
            .peek()
            .ok_or_else(|| "unexpected end of json".to_string())?;
        match b {
            b'n' => {
                self.literal("null")?;
                Ok(Json::Null)
            }
            b't' => {
                self.literal("true")?;
                Ok(Json::Bool(true))
            }
            b'f' => {
                self.literal("false")?;
                Ok(Json::Bool(false))
            }
            b'"' => Ok(Json::Str(self.string()?)),
            b'[' => {
                self.pos += 1;
                self.depth += 1;
                let mut items = Vec::new();
                self.skip_ws();
                if self.peek() == Some(b']') {
                    self.pos += 1;
                    self.depth -= 1;
                    return Ok(Json::Arr(items));
                }
                loop {
                    items.push(self.value()?);
                    self.skip_ws();
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b']') => {
                            self.pos += 1;
                            break;
                        }
                        _ => return Err(format!("expected ',' or ']' at byte {}", self.pos)),
                    }
                }
                self.depth -= 1;
                Ok(Json::Arr(items))
            }
            b'{' => {
                self.pos += 1;
                self.depth += 1;
                let mut pairs = Vec::new();
                self.skip_ws();
                if self.peek() == Some(b'}') {
                    self.pos += 1;
                    self.depth -= 1;
                    return Ok(Json::Obj(pairs));
                }
                loop {
                    self.skip_ws();
                    let key = self.string()?;
                    self.skip_ws();
                    self.expect(b':')?;
                    let value = self.value()?;
                    pairs.push((key, value));
                    self.skip_ws();
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b'}') => {
                            self.pos += 1;
                            break;
                        }
                        _ => return Err(format!("expected ',' or '}}' at byte {}", self.pos)),
                    }
                }
                self.depth -= 1;
                Ok(Json::Obj(pairs))
            }
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(format!("unexpected byte at {}", self.pos)),
        }
    }

    fn literal(&mut self, s: &str) -> Result<(), String> {
        if self.bytes[self.pos..].starts_with(s.as_bytes()) {
            self.pos += s.len();
            Ok(())
        } else {
            Err(format!("invalid literal at byte {}", self.pos))
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let b = self
                .peek()
                .ok_or_else(|| "unterminated string".to_string())?;
            self.pos += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self
                        .peek()
                        .ok_or_else(|| "unterminated escape".to_string())?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let cp = self.hex4()?;
                            if (0xd800..0xdc00).contains(&cp) {
                                if self.peek() == Some(b'\\') {
                                    self.pos += 1;
                                    self.expect(b'u')?;
                                    let lo = self.hex4()?;
                                    if !(0xdc00..0xe000).contains(&lo) {
                                        return Err("invalid surrogate pair".to_string());
                                    }
                                    let c = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
                                    out.push(char::from_u32(c).ok_or("invalid codepoint")?);
                                } else {
                                    return Err("lone surrogate".to_string());
                                }
                            } else {
                                out.push(char::from_u32(cp).ok_or("invalid codepoint")?);
                            }
                        }
                        _ => return Err("invalid escape".to_string()),
                    }
                }
                b if b < 0x20 => return Err("control character in string".to_string()),
                b => {
                    // Decode a UTF-8 sequence.
                    let start = self.pos - 1;
                    let len = utf8_len(b);
                    if len == 0 || start + len > self.bytes.len() {
                        return Err("invalid utf-8".to_string());
                    }
                    let s = std::str::from_utf8(&self.bytes[start..start + len])
                        .map_err(|_| "invalid utf-8".to_string())?;
                    out.push_str(s);
                    self.pos = start + len;
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..4 {
            let b = self
                .peek()
                .ok_or_else(|| "short unicode escape".to_string())?;
            self.pos += 1;
            let d = (b as char)
                .to_digit(16)
                .ok_or_else(|| "bad hex digit".to_string())?;
            v = v * 16 + d;
        }
        Ok(v)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| "bad number".to_string())?;
        if is_float {
            text.parse::<f64>()
                .map(Json::Float)
                .map_err(|_| "bad number".to_string())
        } else if let Ok(v) = text.parse::<i64>() {
            Ok(Json::Int(v))
        } else {
            text.parse::<f64>()
                .map(Json::Float)
                .map_err(|_| "bad number".to_string())
        }
    }
}

fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let src = r#"{"a":1,"b":[true,null,"x\n\u00e9"],"c":-2.5}"#;
        let v = parse(src).unwrap();
        assert_eq!(v.get("a").unwrap().as_i64(), Some(1));
        let arr = v.get("b").unwrap().as_arr().unwrap();
        assert_eq!(arr[2].as_str(), Some("x\n\u{e9}"));
        let rendered = v.render();
        assert_eq!(parse(&rendered).unwrap(), v);
    }

    #[test]
    fn rejects_bad_json() {
        assert!(parse("{").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse("{\"a\":}").is_err());
        assert!(parse("\"\\u12\"").is_err());
        assert!(parse("nul").is_err());
    }

    #[test]
    fn nesting_limit() {
        let src = "[".repeat(200) + &"]".repeat(200);
        assert!(parse(&src).is_err());
    }

    #[test]
    fn escapes_control_characters() {
        let v = Json::str("a\u{1}b\"c");
        let rendered = v.render();
        assert!(rendered.contains("\\u0001"));
        assert_eq!(parse(&rendered).unwrap(), v);
    }
}
