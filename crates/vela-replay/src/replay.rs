//! Replay helpers: run a session deterministically and compare hashes.

use vela_core::runtime::{Limits, Value};

use crate::session::{RunStop, Session, SessionError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayResult {
    pub trace_hash: u64,
    pub state_hash: u64,
    pub memory_hash: u64,
    pub steps: u64,
    pub stop: String,
}

impl ReplayResult {
    pub fn matches(&self, other: &ReplayResult) -> bool {
        self.trace_hash == other.trace_hash
            && self.state_hash == other.state_hash
            && self.memory_hash == other.memory_hash
            && self.steps == other.steps
    }
}

/// Run a session to completion and return its deterministic fingerprints.
pub fn fingerprint(session: &mut Session) -> ReplayResult {
    let stop = session.run_to_completion();
    summarize(session, stop)
}

pub fn summarize(session: &Session, stop: RunStop) -> ReplayResult {
    ReplayResult {
        trace_hash: session.trace_hash(),
        state_hash: session.state_hash(),
        memory_hash: session.store.state_hash(),
        steps: session.exec.steps,
        stop: stop.name().to_string(),
    }
}

/// Re-run a WAT module and collect fingerprints.
pub fn replay_wat(
    src: &str,
    export: &str,
    args: Vec<Value>,
    limits: Limits,
) -> Result<ReplayResult, SessionError> {
    let mut session = Session::from_wat(src, export, args, limits)?;
    Ok(fingerprint(&mut session))
}
