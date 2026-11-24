//! Execution trace records and deterministic hashing.

use std::fmt;

use vela_core::runtime::Hasher;

/// One executed instruction, recorded before it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEvent {
    pub step: u64,
    pub func_addr: u32,
    pub offset: usize,
    pub op: &'static str,
    pub stack_height: usize,
    pub call_depth: usize,
    pub stack_hash: u64,
}

impl TraceEvent {
    pub fn hash_into(&self, h: &mut Hasher) {
        h.write_u64(self.step);
        h.write_u32(self.func_addr);
        h.write_u64(self.offset as u64);
        h.write_bytes(self.op.as_bytes());
        h.write_u64(self.stack_height as u64);
        h.write_u64(self.call_depth as u64);
        h.write_u64(self.stack_hash);
    }
}

#[derive(Clone, Default)]
pub struct Trace {
    pub events: Vec<TraceEvent>,
    /// Trace events dropped after an optional cap (0 in normal operation).
    pub dropped: u64,
}

impl Trace {
    pub fn new() -> Self {
        Trace::default()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn push(&mut self, event: TraceEvent) {
        self.events.push(event);
    }

    pub fn truncate(&mut self, len: usize) {
        self.events.truncate(len);
    }

    pub fn hash(&self) -> u64 {
        let mut h = Hasher::new();
        for e in &self.events {
            e.hash_into(&mut h);
        }
        h.write_u64(self.events.len() as u64);
        h.finish()
    }

    /// Instruction mnemonic histogram, used by the Studio timeline.
    pub fn opcode_counts(&self) -> Vec<(String, usize)> {
        let mut map: std::collections::BTreeMap<&'static str, usize> =
            std::collections::BTreeMap::new();
        for e in &self.events {
            *map.entry(e.op).or_insert(0) += 1;
        }
        map.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    pub fn last(&self) -> Option<&TraceEvent> {
        self.events.last()
    }
}

impl fmt::Debug for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Trace")
            .field("events", &self.events.len())
            .field("hash", &format_args!("{:#x}", self.hash()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(step: u64, op: &'static str) -> TraceEvent {
        TraceEvent {
            step,
            func_addr: 0,
            offset: step as usize,
            op,
            stack_height: 0,
            call_depth: 1,
            stack_hash: step,
        }
    }

    #[test]
    fn hashes_are_sensitive_to_opcodes() {
        let mut a = Trace::new();
        a.push(ev(0, "i32.const"));
        let mut b = Trace::new();
        b.push(ev(0, "i64.const"));
        assert_ne!(a.hash(), b.hash());
        assert_eq!(a.hash(), a.clone().hash());
    }

    #[test]
    fn truncation_restores_prefix_hash() {
        let mut a = Trace::new();
        a.push(ev(0, "a"));
        a.push(ev(1, "b"));
        let prefix = a.hash();
        a.push(ev(2, "c"));
        a.truncate(2);
        assert_eq!(a.hash(), prefix);
    }
}
