//! Loopback-only HTTP/1.1 and SSE server for the Vela Studio.
//!
//! The server is deliberately single-threaded: the interpreter's store uses
//! reference counting, so keeping all state on one thread preserves the
//! soundness guarantees and avoids locking. Every request is bounded.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use vela_core::runtime::{Limits, Value};
use vela_replay::Session;

use crate::api;
use crate::assets;
use crate::json::{self, Json};
use crate::studio::Studio;

pub const TOKEN_HEADER: &str = "x-vela-token";
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    fn token(&self) -> Option<&str> {
        self.query
            .get("token")
            .map(|s| s.as_str())
            .or_else(|| self.headers.get(TOKEN_HEADER).map(|s| s.as_str()))
    }

    fn json(&self) -> Result<Json, String> {
        let text = std::str::from_utf8(&self.body).map_err(|_| "body is not UTF-8".to_string())?;
        json::parse(text)
    }
}

struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn json(value: &Json, status: u16) -> Self {
        Response {
            status,
            content_type: "application/json; charset=utf-8",
            body: value.render().into_bytes(),
        }
    }

    fn error(status: u16, message: impl Into<String>) -> Self {
        Response::json(
            &Json::obj(vec![("error", Json::str(message.into()))]),
            status,
        )
    }

    fn text(body: &'static str, content_type: &'static str) -> Self {
        Response {
            status: 200,
            content_type,
            body: body.as_bytes().to_vec(),
        }
    }

    fn empty(status: u16) -> Self {
        Response { status, content_type: "text/plain; charset=utf-8", body: Vec::new() }
    }
}

/// Bind to an ephemeral loopback port, print the URL, optionally open a
/// browser, and serve until `/api/shutdown` is called.
pub fn run_server(mut studio: Studio, token: String, open_browser: bool) -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let addr = listener.local_addr()?;
    let url = format!("http://{addr}/?token={token}");
    println!("VELA_STUDIO_URL={url}");
    let _ = io::stdout().flush();
    if open_browser {
        launch_browser(&url);
    }
    let origin = format!("http://{addr}");
    let mut shutdown = false;
    for incoming in listener.incoming() {
        if shutdown {
            break;
        }
        let mut stream = match incoming {
            Ok(s) => s,
            Err(_) => continue,
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(15)));
        let request = match read_request(&mut stream) {
            Ok(r) => r,
            Err(e) => {
                let _ = write_response(&mut stream, &Response::error(400, e));
                continue;
            }
        };
        match handle(&mut studio, &request, &mut stream, &token, &origin) {
            Ok(stop) => shutdown = stop,
            Err(_) => {
                let _ = write_response(
                    &mut stream,
                    &Response::error(500, "internal server error"),
                );
            }
        }
    }
    Ok(())
}

fn handle(
    studio: &mut Studio,
    req: &Request,
    stream: &mut TcpStream,
    token: &str,
    origin: &str,
) -> io::Result<bool> {
    let path = req.path.as_str();
    // Static assets are same-origin and carry no secrets.
    match path {
        "/favicon.ico" => {
            return write_response(stream, &Response::empty(204)).map(|_| false);
        }
        "/app.js" => {
            return write_response(stream, &Response::text(assets::APP_JS, "text/javascript; charset=utf-8"))
                .map(|_| false);
        }
        "/style.css" => {
            return write_response(stream, &Response::text(assets::STYLE_CSS, "text/css; charset=utf-8"))
                .map(|_| false);
        }
        "/" | "/index.html" => {
            if !authorized(req, token) {
                return write_response(stream, &Response::error(403, "missing or invalid token"))
                    .map(|_| false);
            }
            return write_response(stream, &Response::text(assets::INDEX_HTML, "text/html; charset=utf-8"))
                .map(|_| false);
        }
        _ => {}
    }
    if !authorized(req, token) {
        return write_response(stream, &Response::error(403, "missing or invalid token"))
            .map(|_| false);
    }
    if req.method == "POST" && !origin_allowed(req, origin) {
        return write_response(stream, &Response::error(403, "cross-origin request rejected"))
            .map(|_| false);
    }

    let response = match (req.method.as_str(), path) {
        ("GET", "/api/state") => {
            let start = req
                .query
                .get("start")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            Response::json(&api::state_json(studio, start), 200)
        }
        ("GET", "/api/health") => Response::json(
            &Json::obj(vec![
                ("ok", Json::Bool(true)),
                ("version", Json::str(env!("CARGO_PKG_VERSION"))),
                ("step", Json::Int(studio.session().map(|s| s.exec.steps as i64).unwrap_or(0))),
            ]),
            200,
        ),
        ("GET", "/api/trace.json") => Response::json(&api::trace_json(studio), 200),
        ("GET", "/api/spec") => Response::json(&api::spec_json(None), 200),
        ("GET", "/api/exports") => Response::json(&api::exports_of(&studio.source), 200),
        ("GET", "/api/stream") => {
            let _ = stream_trace(studio, stream);
            return Ok(false);
        }
        ("POST", "/api/load") => match req.json() {
            Ok(body) => {
                let source = body
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&studio.source)
                    .to_string();
                let export = body
                    .get("export")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let args = parse_args(body.get("args"));
                match studio.load(source, export, args) {
                    Ok(()) => Response::json(&api::state_json(studio, 0), 200),
                    Err(e) => Response::json(
                        &Json::obj(vec![
                            ("error", Json::str(e)),
                            ("state", api::state_json(studio, 0)),
                        ]),
                        400,
                    ),
                }
            }
            Err(e) => Response::error(400, e),
        },
        ("POST", "/api/action") => match req.json() {
            Ok(body) => {
                let action = body.get("action").and_then(|v| v.as_str()).unwrap_or("");
                let target = body.get("target").and_then(|v| v.as_i64()).map(|v| v.max(0) as u64);
                match studio.action(action, target) {
                    Ok(()) => Response::json(&api::state_json(studio, 0), 200),
                    Err(e) => Response::json(
                        &Json::obj(vec![
                            ("error", Json::str(e)),
                            ("state", api::state_json(studio, 0)),
                        ]),
                        400,
                    ),
                }
            }
            Err(e) => Response::error(400, e),
        },
        ("POST", "/api/breakpoints") => match req.json() {
            Ok(body) => {
                let action = body.get("action").and_then(|v| v.as_str()).unwrap_or("add");
                let result = match action {
                    "clear" => {
                        studio.clear_breakpoints();
                        Ok(())
                    }
                    "host_on" => studio.add_breakpoint("host"),
                    "host_off" => {
                        studio.remove_host_breakpoint();
                        Ok(())
                    }
                    "add_memory" => match body.get("address").and_then(|v| v.as_i64()) {
                        Some(a) => studio.add_memory_breakpoint(a.max(0) as u32),
                        None => Err("memory breakpoint needs an address".to_string()),
                    },
                    "toggle_instruction" => {
                        let func = body
                            .get("func")
                            .and_then(|v| v.as_i64())
                            .map(|v| v as u32)
                            .or_else(|| studio.current_func_addr());
                        let offset = body.get("offset").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
                        if let (Some(func), Some(session)) = (func, studio.session.as_mut()) {
                            let bp = vela_replay::Breakpoint::Instruction { func_addr: func, offset };
                            if session.breakpoints.contains(&bp) {
                                session.remove_breakpoint(&bp);
                            } else {
                                session.add_breakpoint(bp);
                            }
                            Ok(())
                        } else {
                            Err("no current function".to_string())
                        }
                    }
                    "add_function" => studio.add_breakpoint("function"),
                    "remove" => {
                        let index = body.get("index").and_then(|v| v.as_i64()).unwrap_or(-1);
                        if index >= 0 {
                            if let Some(session) = studio.session.as_mut() {
                                let index = index as usize;
                                if index < session.breakpoints.len() {
                                    session.breakpoints.remove(index);
                                }
                            }
                            Ok(())
                        } else {
                            Err("remove needs an index".to_string())
                        }
                    }
                    _ => studio.add_breakpoint("instruction"),
                };
                match result {
                    Ok(()) => Response::json(&api::state_json(studio, 0), 200),
                    Err(e) => Response::error(400, e),
                }
            }
            Err(e) => Response::error(400, e),
        },
        ("POST", "/api/spec") => {
            let custom = req
                .json()
                .ok()
                .and_then(|b| b.get("source").and_then(|v| v.as_str()).map(|s| s.to_string()));
            Response::json(&api::spec_json(custom.as_deref()), 200)
        }
        ("POST", "/api/diff") => match req.json() {
            Ok(body) => {
                let export = body
                    .get("export")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&studio.export)
                    .to_string();
                let a = parse_i32_list(body.get("args_a"));
                let b = parse_i32_list(body.get("args_b"));
                Response::json(&api::diff_json(&studio.source, &export, a, b), 200)
            }
            Err(e) => Response::error(400, e),
        },
        ("POST", "/api/shutdown") => {
            let _ = write_response(stream, &Response::json(&Json::obj(vec![("ok", Json::Bool(true))]), 200));
            return Ok(true);
        }
        (_, "/api/state" | "/api/health" | "/api/trace.json" | "/api/spec" | "/api/exports" | "/api/stream"
            | "/api/load" | "/api/action" | "/api/breakpoints" | "/api/diff" | "/api/shutdown") => {
            Response::error(405, "method not allowed")
        }
        (_, p) if p.starts_with("/api/") => Response::error(404, "unknown API route"),
        _ => Response::error(404, "not found"),
    };
    write_response(stream, &response)?;
    Ok(false)
}

fn origin_allowed(req: &Request, origin: &str) -> bool {
    match req.headers.get("origin") {
        None => true,
        Some(o) => {
            if o == origin {
                return true;
            }
            // The page can be opened via localhost instead of 127.0.0.1.
            if let Some(port) = origin.rsplit(':').next() {
                let localhost = format!("http://localhost:{port}");
                return o == &localhost;
            }
            false
        }
    }
}

fn authorized(req: &Request, token: &str) -> bool {
    req.token().map(|t| t == token).unwrap_or(false)
}

fn parse_args(value: Option<&Json>) -> Vec<Value> {
    match value.and_then(|v| v.as_arr()) {
        Some(items) => items
            .iter()
            .filter_map(|v| v.as_i64())
            .map(|v| Value::I32(v as i32))
            .collect(),
        None => Vec::new(),
    }
}

fn parse_i32_list(value: Option<&Json>) -> Vec<i32> {
    match value.and_then(|v| v.as_arr()) {
        Some(items) => items.iter().filter_map(|v| v.as_i64()).map(|v| v as i32).collect(),
        None => Vec::new(),
    }
}

fn stream_trace(studio: &Studio, stream: &mut TcpStream) -> io::Result<()> {
    let source = studio.source.clone();
    let export = studio.export.clone();
    let args = studio.args.clone();
    let limits = Limits { max_steps: 5_000, ..studio.limits };
    let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n";
    stream.write_all(headers.as_bytes())?;
    stream.flush()?;

    let mut session = match Session::from_wat(&source, &export, args, limits) {
        Ok(s) => s,
        Err(e) => {
            let payload = Json::obj(vec![("error", Json::str(e.to_string()))]).render();
            let _ = write!(stream, "event: error\ndata: {payload}\n\n");
            return Ok(());
        }
    };
    let mut last_step = 0u64;
    let mut sent = 0usize;
    loop {
        let result = session.step_forward();
        if let Some(e) = session.trace.last() {
            if e.step >= last_step {
                last_step = e.step;
                let payload = Json::obj(vec![
                    ("step", Json::Int(e.step as i64)),
                    ("func", Json::Int(e.func_addr as i64)),
                    ("offset", Json::Int(e.offset as i64)),
                    ("op", Json::str(e.op)),
                    ("stack", Json::Int(e.stack_height as i64)),
                    ("depth", Json::Int(e.call_depth as i64)),
                ])
                .render();
                if write!(stream, "event: step\ndata: {payload}\n\n").is_err() {
                    return Ok(());
                }
                sent += 1;
                if sent % 32 == 0 {
                    stream.flush()?;
                }
            }
        }
        match result {
            vela_core::runtime::StepResult::Running => {
                if sent >= 2_000 {
                    break;
                }
            }
            vela_core::runtime::StepResult::Finished => break,
            vela_core::runtime::StepResult::Trapped(_) => break,
        }
    }
    let summary = Json::obj(vec![
        ("steps", Json::Int(session.exec.steps as i64)),
        ("trace_hash", Json::str(format!("{:016x}", session.trace_hash()))),
        ("state_hash", Json::str(format!("{:016x}", session.state_hash()))),
        ("finished", Json::Bool(session.exec.finished)),
        (
            "trap",
            match &session.exec.trap {
                Some(t) => Json::str(t.to_string()),
                None => Json::Null,
            },
        ),
    ]);
    let _ = write!(stream, "event: done\ndata: {}\n\n", summary.render());
    let _ = stream.flush();
    Ok(())
}

fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let n = stream.read(&mut chunk).map_err(|e| format!("read error: {e}"))?;
        if n == 0 {
            return Err("connection closed before headers".to_string());
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > MAX_HEADER_BYTES + MAX_BODY_BYTES {
            return Err("request too large".to_string());
        }
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            if pos + 4 > MAX_HEADER_BYTES {
                return Err("headers too large".to_string());
            }
            header_end = pos + 4;
            break;
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err("headers too large".to_string());
        }
    }
    let head = std::str::from_utf8(&buf[..header_end])
        .map_err(|_| "headers are not UTF-8".to_string())?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or_else(|| "empty request".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or_else(|| "missing method".to_string())?.to_string();
    let target = parts.next().ok_or_else(|| "missing target".to_string())?.to_string();
    let version = parts.next().unwrap_or("HTTP/1.1");
    if !version.starts_with("HTTP/1.") {
        return Err("unsupported HTTP version".to_string());
    }
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), parse_query(q)),
        None => (target, HashMap::new()),
    };
    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err("body too large".to_string());
    }
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).map_err(|e| format!("read error: {e}"))?;
        if n == 0 {
            return Err("connection closed before body".to_string());
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);
    Ok(Request { method, path, query, headers, body })
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        out.insert(percent_decode(k), percent_decode(v));
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn write_response(stream: &mut TcpStream, res: &Response) -> io::Result<()> {
    let reason = match res.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let csp = if res.content_type.starts_with("text/html") {
        "Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'none'\r\n"
    } else {
        ""
    };
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\n{}{}",
        res.status,
        reason,
        res.content_type,
        res.body.len(),
        csp,
        "\r\n"
    );
    if res.status == 204 {
        head = "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_string();
    }
    stream.write_all(head.as_bytes())?;
    stream.write_all(&res.body)?;
    stream.flush()
}

fn launch_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]).arg(url);
        let _ = c.spawn();
        return;
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new(program).arg(url).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new(program).arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    #[allow(unreachable_code)]
    {
        let _ = command;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    fn request(method: &str, path: &str, token: Option<&str>, origin: Option<&str>) -> Request {
        let mut headers = HashMap::new();
        if let Some(t) = token {
            headers.insert(TOKEN_HEADER.to_string(), t.to_string());
        }
        if let Some(o) = origin {
            headers.insert("origin".to_string(), o.to_string());
        }
        Request {
            method: method.to_string(),
            path: path.to_string(),
            query: HashMap::new(),
            headers,
            body: Vec::new(),
        }
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("%2F%2f"), "//");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn query_parsing() {
        let q = parse_query("token=abc&start=5&empty=&flag");
        assert_eq!(q["token"], "abc");
        assert_eq!(q["start"], "5");
        assert_eq!(q["empty"], "");
        assert_eq!(q["flag"], "");
    }

    #[test]
    fn authorization_accepts_query_or_header() {
        let token = "deadbeef";
        let mut r = request("GET", "/api/state", Some(token), None);
        assert!(authorized(&r, token));
        assert!(!authorized(&r, "other"));
        r.headers.clear();
        assert!(!authorized(&r, token));
        r.query.insert("token".into(), token.into());
        assert!(authorized(&r, token));
    }

    #[test]
    fn origin_policy_blocks_cross_site_posts() {
        let origin = "http://127.0.0.1:9000";
        assert!(origin_allowed(&request("POST", "/api/action", Some("t"), None), origin));
        assert!(origin_allowed(
            &request("POST", "/api/action", Some("t"), Some("http://127.0.0.1:9000")),
            origin
        ));
        assert!(origin_allowed(
            &request("POST", "/api/action", Some("t"), Some("http://localhost:9000")),
            origin
        ));
        assert!(!origin_allowed(
            &request("POST", "/api/action", Some("t"), Some("http://evil.example")),
            origin
        ));
    }

    #[test]
    fn reads_request_with_body() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            let body = "{\"action\":\"step\"}";
            write!(
                stream,
                "POST /api/action?token=abc HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            stream.flush().unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        let req = read_request(&mut stream).unwrap();
        handle.join().unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/api/action");
        assert_eq!(req.query["token"], "abc");
        assert_eq!(req.headers["host"], "localhost");
        assert_eq!(req.body, b"{\"action\":\"step\"}");
    }

    #[test]
    fn rejects_oversized_headers() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            let junk = "X".repeat(MAX_HEADER_BYTES + 10);
            let _ = write!(stream, "GET / HTTP/1.1\r\nX: {junk}\r\n\r\n");
            let _ = stream.flush();
        });
        let (mut stream, _) = listener.accept().unwrap();
        let err = read_request(&mut stream).unwrap_err();
        handle.join().unwrap();
        // The exact error depends on how much of the oversized header the
        // client manages to send before the server rejects or the socket
        // resets; any rejection is acceptable.
        assert!(!err.is_empty());
    }

    #[test]
    fn rejects_truncated_bodies() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            let _ = write!(
                stream,
                "POST /api/action HTTP/1.1\r\nContent-Length: 100\r\n\r\nabc"
            );
            stream.shutdown(std::net::Shutdown::Write).unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        let err = read_request(&mut stream).unwrap_err();
        handle.join().unwrap();
        assert!(err.contains("closed before body"), "{err}");
    }
}
