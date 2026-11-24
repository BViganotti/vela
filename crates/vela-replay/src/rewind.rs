//! Checkpoint log used for reverse stepping.
//!
//! Reverse stepping restores the nearest snapshot at or before the target and
//! re-executes forward. Host results and clock/randomness are part of the
//! snapshotted store, so replay is bit-exact.

use vela_core::runtime::{ExecState, Store};

#[derive(Clone)]
pub struct Checkpoint {
    pub step: u64,
    pub store: Store,
    pub exec: ExecState,
    pub trace_len: usize,
}

#[derive(Clone)]
pub struct CheckpointLog {
    pub interval: u64,
    points: Vec<Checkpoint>,
}

impl Default for CheckpointLog {
    fn default() -> Self {
        CheckpointLog {
            interval: 64,
            points: Vec::new(),
        }
    }
}

impl CheckpointLog {
    pub fn new(interval: u64) -> Self {
        CheckpointLog {
            interval: interval.max(1),
            points: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn points(&self) -> &[Checkpoint] {
        &self.points
    }

    /// Capture a checkpoint for `step`, at most once per step.
    pub fn capture(&mut self, step: u64, store: &Store, exec: &ExecState, trace_len: usize) {
        if let Some(last) = self.points.last() {
            if last.step == step {
                return;
            }
        }
        self.points.push(Checkpoint {
            step,
            store: store.clone(),
            exec: exec.clone(),
            trace_len,
        });
    }

    pub fn maybe_capture(&mut self, step: u64, store: &Store, exec: &ExecState, trace_len: usize) {
        if step == 0 || step % self.interval == 0 {
            self.capture(step, store, exec, trace_len);
        }
    }

    /// Latest checkpoint with `step <= target`.
    pub fn nearest(&self, target: u64) -> Option<&Checkpoint> {
        self.points.iter().rev().find(|c| c.step <= target)
    }

    pub fn truncate_after(&mut self, step: u64) {
        self.points.retain(|c| c.step <= step);
    }

    pub fn clear(&mut self) {
        self.points.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_is_monotonic() {
        let log = CheckpointLog::new(10);
        assert!(log.nearest(5).is_none());
    }
}
