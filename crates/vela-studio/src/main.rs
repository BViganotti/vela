//! Vela Studio launcher: starts a loopback HTTP/SSE server and opens a browser.
//!
//! No terminal interaction is required. The URL (with a random auth token) is
//! printed on one line for tooling and automatically opened in the browser.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use vela_studio::{run_server, Studio};

fn main() {
    let token = random_token();
    let studio = Studio::new();
    let open = std::env::var("VELA_NO_OPEN").map(|v| v != "1").unwrap_or(true);
    if let Err(e) = run_server(studio, token, open) {
        eprintln!("vela-studio: {e}");
        std::process::exit(1);
    }
}

fn random_token() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut x = nanos ^ pid.rotate_left(17) ^ c.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let mut out = String::with_capacity(32);
    for _ in 0..4 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push_str(&format!("{x:016x}"));
    }
    out
}
