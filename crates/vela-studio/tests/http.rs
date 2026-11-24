//! End-to-end HTTP tests against the real Studio binary.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Studio {
    child: Child,
    addr: String,
    token: String,
}

impl Drop for Studio {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start() -> Studio {
    let mut child = Command::new(env!("CARGO_BIN_EXE_vela-studio"))
        .env("VELA_NO_OPEN", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn studio");
    let stdout = child.stdout.as_mut().unwrap();
    let mut line = String::new();
    // Read bytes until newline, avoiding a blocking read_to_string.
    let mut buf = [0u8; 1];
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => {
                if buf[0] == b'\n' {
                    break;
                }
                line.push(buf[0] as char);
            }
            Err(_) => break,
        }
    }
    let url = line
        .split("VELA_STUDIO_URL=")
        .nth(1)
        .expect("studio prints a URL");
    let (addr, token) = url.split_once("/?token=").expect("url has token");
    let addr = addr.trim_start_matches("http://").to_string();
    Studio {
        child,
        addr,
        token: token.trim().to_string(),
    }
}

fn request(
    studio: &Studio,
    method: &str,
    path: &str,
    with_token: bool,
    body: Option<&str>,
    origin: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(&studio.addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let body = body.unwrap_or("");
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    if with_token {
        head.push_str(&format!("X-Vela-Token: {}\r\n", studio.token));
    }
    if let Some(o) = origin {
        head.push_str(&format!("Origin: {o}\r\n"));
    }
    if !body.is_empty() {
        head.push_str("Content-Type: application/json\r\n");
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    head.push_str(body);
    stream.write_all(head.as_bytes()).unwrap();
    stream.flush().unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let text = String::from_utf8_lossy(&response).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (status, body)
}

#[test]
fn health_state_and_actions() {
    let studio = start();
    let (status, body) = request(&studio, "GET", "/api/health", true, None, None);
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"ok\":true"));
    assert!(body.contains("\"version\""));

    let (status, body) = request(&studio, "GET", "/api/state", true, None, None);
    assert_eq!(status, 200);
    assert!(body.contains("\"export\":\"main\""));
    assert!(body.contains("\"disasm\""));

    let (status, body) = request(
        &studio,
        "POST",
        "/api/action",
        true,
        Some("{\"action\":\"step\"}"),
        None,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"step\":1"), "{body}");

    let (status, _) = request(&studio, "POST", "/api/shutdown", true, Some("{}"), None);
    assert_eq!(status, 200);
}

#[test]
fn authentication_and_origin_are_enforced() {
    let studio = start();
    let (status, _) = request(&studio, "GET", "/api/state", false, None, None);
    assert_eq!(status, 403);
    let (status, _) = request(&studio, "GET", "/api/state", true, None, None);
    assert_eq!(status, 200);

    let (status, _) = request(
        &studio,
        "POST",
        "/api/action",
        true,
        Some("{\"action\":\"step\"}"),
        Some("http://attacker.example"),
    );
    assert_eq!(status, 403);
    let (status, _) = request(
        &studio,
        "POST",
        "/api/action",
        true,
        Some("{\"action\":\"step\"}"),
        Some(&format!("http://{}", studio.addr)),
    );
    assert_eq!(status, 200);

    let (status, _) = request(&studio, "POST", "/api/health", true, Some("{}"), None);
    assert_eq!(status, 405);
}

#[test]
fn spec_and_diff_endpoints() {
    let studio = start();
    let (status, body) = request(&studio, "GET", "/api/spec", true, None, None);
    assert_eq!(status, 200);
    assert!(body.contains("\"passed\":41"), "{body}");

    let diff = r#"{"export":"main","args_a":[10],"args_b":[12]}"#;
    let (status, body) = request(&studio, "POST", "/api/diff", true, Some(diff), None);
    assert_eq!(status, 200);
    assert!(body.contains("\"diverged\":true"), "{body}");
    assert!(body.contains("\"step\""));

    let (status, body) = request(&studio, "GET", "/api/trace.json", true, None, None);
    assert_eq!(status, 200);
    assert!(body.contains("\"events\""));
}

#[test]
fn sse_stream_emits_steps_and_done() {
    let studio = start();
    let mut stream = TcpStream::connect(&studio.addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    write!(
        stream,
        "GET /api/stream?token={} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        studio.token
    )
    .unwrap();
    stream.flush().unwrap();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(13).any(|w| w == b"event: done\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&buf);
    assert!(text.contains("200 OK"), "{text}");
    assert!(text.contains("text/event-stream"));
    assert!(text.contains("event: step"), "no step events");
    assert!(text.contains("event: done"), "no done event");
    assert!(text.contains("\"steps\""));
}

#[test]
fn invalid_module_load_returns_400_and_keeps_state() {
    let studio = start();
    let body = r#"{"source":"(module (func (result i32) i64.const 0))","export":"f","args":[]}"#;
    let (status, text) = request(&studio, "POST", "/api/load", true, Some(body), None);
    assert_eq!(status, 400);
    assert!(text.contains("error"), "{text}");
    let (status, state) = request(&studio, "GET", "/api/state", true, None, None);
    assert_eq!(status, 200);
    assert!(state.contains("\"loaded\":true"));
}
