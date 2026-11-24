//! Linear memories with bounds checks and growth limits.

use crate::runtime::trap::Trap;
use crate::types::{Limits, PAGE_SIZE};

pub const DEFAULT_MAX_MEMORY_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct MemoryInst {
    data: Vec<u8>,
    min: u32,
    max: Option<u32>,
    max_bytes: usize,
}

impl MemoryInst {
    pub fn new(limits: Limits) -> Self {
        Self::try_new(limits, DEFAULT_MAX_MEMORY_BYTES).unwrap_or_else(|| MemoryInst {
            data: Vec::new(),
            min: 0,
            max: Some(0),
            max_bytes: 0,
        })
    }

    /// Allocate a memory, refusing to reserve more than `max_bytes`.
    pub fn try_new(limits: Limits, max_bytes: usize) -> Option<Self> {
        let size = limits.min as usize * PAGE_SIZE;
        if size > max_bytes {
            return None;
        }
        let mut data = Vec::new();
        if data.try_reserve_exact(size).is_err() {
            return None;
        }
        data.resize(size, 0);
        Some(MemoryInst {
            data,
            min: limits.min,
            max: limits.max,
            max_bytes,
        })
    }

    pub fn size_pages(&self) -> u32 {
        (self.data.len() / PAGE_SIZE) as u32
    }

    pub fn limits(&self) -> Limits {
        Limits::new(self.min, self.max)
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Grow by `delta` pages. Returns the previous size or `None` on failure.
    pub fn grow(&mut self, delta: u32) -> Option<u32> {
        let old = self.size_pages();
        let new = old.checked_add(delta)?;
        if new > crate::types::MAX_PAGES {
            return None;
        }
        if let Some(max) = self.max {
            if new > max {
                return None;
            }
        }
        let new_len = new as usize * PAGE_SIZE;
        if new_len > self.max_bytes {
            return None;
        }
        // Reserve before zero-filling so allocation failure is reported as a
        // failed grow rather than a panic.
        if self
            .data
            .try_reserve_exact(new_len - self.data.len())
            .is_err()
        {
            return None;
        }
        self.data.resize(new_len, 0);
        Some(old)
    }

    fn range(&self, addr: u32, offset: u32, width: usize) -> Result<usize, Trap> {
        let start = (addr as u64) + (offset as u64);
        let end = start + width as u64;
        if end > self.data.len() as u64 {
            return Err(Trap::MemoryOutOfBounds);
        }
        Ok(start as usize)
    }

    pub fn read(&self, addr: u32, offset: u32, buf: &mut [u8]) -> Result<(), Trap> {
        let start = self.range(addr, offset, buf.len())?;
        buf.copy_from_slice(&self.data[start..start + buf.len()]);
        Ok(())
    }

    pub fn write(&mut self, addr: u32, offset: u32, bytes: &[u8]) -> Result<(), Trap> {
        let start = self.range(addr, offset, bytes.len())?;
        self.data[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    pub fn load_u8(&self, addr: u32, offset: u32) -> Result<u8, Trap> {
        let i = self.range(addr, offset, 1)?;
        Ok(self.data[i])
    }

    pub fn init(&mut self, dst: u32, src: &[u8], src_off: u32, len: u32) -> Result<(), Trap> {
        let s = src_off as u64;
        let d = dst as u64;
        let n = len as u64;
        if s + n > src.len() as u64 || d + n > self.data.len() as u64 {
            return Err(Trap::MemoryOutOfBounds);
        }
        let (s, d, n) = (s as usize, d as usize, n as usize);
        self.data[d..d + n].copy_from_slice(&src[s..s + n]);
        Ok(())
    }

    pub fn copy(&mut self, dst: u32, src: u32, len: u32) -> Result<(), Trap> {
        let d = dst as u64;
        let s = src as u64;
        let n = len as u64;
        if s + n > self.data.len() as u64 || d + n > self.data.len() as u64 {
            return Err(Trap::MemoryOutOfBounds);
        }
        let (d, s, n) = (d as usize, s as usize, n as usize);
        self.data.copy_within(s..s + n, d);
        Ok(())
    }

    pub fn fill(&mut self, dst: u32, val: u8, len: u32) -> Result<(), Trap> {
        let d = dst as u64;
        let n = len as u64;
        if d + n > self.data.len() as u64 {
            return Err(Trap::MemoryOutOfBounds);
        }
        let d = d as usize;
        self.data[d..d + n as usize].fill(val);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_to_zero() {
        let m = MemoryInst::new(Limits::new(1, None));
        assert_eq!(m.len(), PAGE_SIZE);
        assert!(m.data().iter().all(|b| *b == 0));
    }

    #[test]
    fn grows_and_respects_max() {
        let mut m = MemoryInst::new(Limits::new(1, Some(2)));
        assert_eq!(m.grow(1), Some(1));
        assert_eq!(m.grow(1), None);
        assert_eq!(m.size_pages(), 2);
    }

    #[test]
    fn bounds_are_checked_without_overflow() {
        let mut m = MemoryInst::new(Limits::new(1, None));
        assert_eq!(m.load_u8(u32::MAX, 1), Err(Trap::MemoryOutOfBounds));
        assert_eq!(m.write(u32::MAX, 0, &[1, 2]), Err(Trap::MemoryOutOfBounds));
        assert_eq!(m.fill(0, 0, u32::MAX), Err(Trap::MemoryOutOfBounds));
    }

    #[test]
    fn copy_is_memmove() {
        let mut m = MemoryInst::new(Limits::new(1, None));
        m.write(0, 0, &[1, 2, 3, 4]).unwrap();
        m.copy(1, 0, 3).unwrap();
        assert_eq!(&m.data()[..5], &[1, 1, 2, 3, 0]);
    }
}
