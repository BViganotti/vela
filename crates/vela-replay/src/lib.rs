//! Deterministic recording, replay and reverse stepping for Vela.

pub mod diff;
pub mod record;
pub mod replay;
pub mod rewind;
pub mod session;

pub use diff::{first_divergence, Divergence};
pub use record::{Trace, TraceEvent};
pub use replay::{fingerprint, replay_wat, ReplayResult};
pub use rewind::{Checkpoint, CheckpointLog};
pub use session::{Breakpoint, ReplayError, RunStop, Session, SessionError, SessionStatus};
