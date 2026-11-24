//! Bounded little-endian reader for the WebAssembly binary format.

use crate::error::DecodeError;

/// Maximum bytes accepted for a module by default (64 MiB).
pub const MAX_MODULE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn err<T>(&self, message: impl Into<String>) -> Result<T, DecodeError> {
        Err(DecodeError::new(self.pos, message))
    }

    pub fn read_u8(&mut self) -> Result<u8, DecodeError> {
        match self.data.get(self.pos) {
            Some(b) => {
                self.pos += 1;
                Ok(*b)
            }
            None => self.err("unexpected end of input"),
        }
    }

    pub fn peek_u8(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if n > self.remaining() {
            return self.err("unexpected end of input");
        }
        let start = self.pos;
        self.pos += n;
        Ok(&self.data[start..start + n])
    }

    /// Read an unsigned LEB128 value constrained to 32 bits.
    pub fn read_u32(&mut self) -> Result<u32, DecodeError> {
        let mut result: u32 = 0;
        for i in 0..5 {
            let b = self.read_u8()?;
            if i == 4 && (b & 0x70) != 0 {
                return self.err("integer too large");
            }
            result |= ((b & 0x7f) as u32) << (i * 7);
            if b & 0x80 == 0 {
                return Ok(result);
            }
        }
        self.err("integer representation too long")
    }

    /// Read a signed LEB128 value constrained to 32 bits.
    pub fn read_i32(&mut self) -> Result<i32, DecodeError> {
        let mut result: u64 = 0;
        for i in 0..5 {
            let b = self.read_u8()?;
            if i == 4 {
                let sign = b & 0x08 != 0;
                let high = b & 0x70;
                if (sign && high != 0x70) || (!sign && high != 0) {
                    return self.err("integer too large");
                }
            }
            result |= ((b & 0x7f) as u64) << (i * 7);
            if b & 0x80 == 0 {
                let bits = (i + 1) * 7;
                if i < 4 && (b & 0x40) != 0 {
                    result |= u64::MAX << bits;
                }
                return Ok(result as u32 as i32);
            }
        }
        self.err("integer representation too long")
    }

    /// Read a signed LEB128 value constrained to 64 bits.
    pub fn read_i64(&mut self) -> Result<i64, DecodeError> {
        let mut result: u64 = 0;
        for i in 0..10 {
            let b = self.read_u8()?;
            if i == 9 {
                let sign = b & 0x01 != 0;
                let high = b & 0x7e;
                if (sign && high != 0x7e) || (!sign && high != 0) {
                    return self.err("integer too large");
                }
            }
            result |= ((b & 0x7f) as u64) << (i * 7);
            if b & 0x80 == 0 {
                let bits = (i + 1) * 7;
                if i < 9 && (b & 0x40) != 0 {
                    result |= u64::MAX << bits;
                }
                return Ok(result as i64);
            }
        }
        self.err("integer representation too long")
    }

    /// Read an unsigned 33-bit LEB value used by block type indices.
    pub fn read_s33(&mut self) -> Result<i64, DecodeError> {
        let mut result: u64 = 0;
        for i in 0..5 {
            let b = self.read_u8()?;
            if i == 4 {
                let sign = b & 0x10 != 0;
                let high = b & 0x60;
                if (sign && high != 0x60) || (!sign && high != 0) {
                    return self.err("integer too large");
                }
            }
            result |= ((b & 0x7f) as u64) << (i * 7);
            if b & 0x80 == 0 {
                let bits = (i + 1) * 7;
                if i < 4 && (b & 0x40) != 0 {
                    result |= u64::MAX << bits;
                }
                return Ok(result as i64);
            }
        }
        self.err("integer representation too long")
    }

    pub fn read_f32_bits(&mut self) -> Result<u32, DecodeError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_f64_bits(&mut self) -> Result<u64, DecodeError> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read a length-prefixed UTF-8 name.
    pub fn read_name(&mut self) -> Result<String, DecodeError> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_bytes(len)?;
        match std::str::from_utf8(bytes) {
            Ok(s) => Ok(s.to_string()),
            Err(_) => self.err("malformed UTF-8 encoding"),
        }
    }

    pub fn read_vec_count(&mut self, what: &str, max: usize) -> Result<usize, DecodeError> {
        let n = self.read_u32()? as usize;
        if n > max || n > self.remaining() {
            return self.err(format!("{what} count out of bounds"));
        }
        Ok(n)
    }

    /// Split off `len` bytes as an independent reader.
    pub fn sub_reader(&mut self, len: usize) -> Result<Reader<'a>, DecodeError> {
        let bytes = self.read_bytes(len)?;
        Ok(Reader::new(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_leb() {
        assert_eq!(Reader::new(&[0x00]).read_u32().unwrap(), 0);
        assert_eq!(Reader::new(&[0x7f]).read_u32().unwrap(), 127);
        assert_eq!(Reader::new(&[0x80, 0x01]).read_u32().unwrap(), 128);
        assert_eq!(
            Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x0f])
                .read_u32()
                .unwrap(),
            u32::MAX
        );
    }

    #[test]
    fn unsigned_leb_rejects_overflow() {
        assert!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x1f])
            .read_u32()
            .is_err());
        assert!(Reader::new(&[0x80; 6]).read_u32().is_err());
    }

    #[test]
    fn signed_leb() {
        assert_eq!(Reader::new(&[0x7f]).read_i32().unwrap(), -1);
        assert_eq!(Reader::new(&[0x40]).read_i32().unwrap(), -64);
        assert_eq!(Reader::new(&[0x80, 0x7f]).read_i32().unwrap(), -128);
        assert_eq!(
            Reader::new(&[0xff, 0xff, 0xff, 0xff, 0x07])
                .read_i32()
                .unwrap(),
            i32::MAX
        );
        assert_eq!(
            Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x78])
                .read_i32()
                .unwrap(),
            i32::MIN
        );
    }

    #[test]
    fn signed_leb_64() {
        assert_eq!(Reader::new(&[0x7f]).read_i64().unwrap(), -1);
        assert_eq!(
            Reader::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00])
                .read_i64()
                .unwrap(),
            i64::MAX
        );
        assert_eq!(
            Reader::new(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x7f])
                .read_i64()
                .unwrap(),
            i64::MIN
        );
    }

    #[test]
    fn truncated_input_errors() {
        let mut r = Reader::new(&[0x01]);
        assert!(r.read_bytes(2).is_err());
        let mut r = Reader::new(&[]);
        assert!(r.read_u8().is_err());
        let mut r = Reader::new(&[0x03, b'a']);
        assert!(r.read_name().is_err());
    }
}
