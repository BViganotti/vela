# Vela Studio

Vela Studio is a local browser workbench. It is not a terminal UI and not a
remote service. The Rust binary starts an embedded HTTP/1.1 + SSE server bound
to an ephemeral port on `127.0.0.1`, opens the default browser and exits when
the process is terminated or `/api/shutdown` is called.

## Security model

- Bind address is fixed to `127.0.0.1`; there is no option to listen publicly.
- A 128-bit token is generated at startup from time, pid and a counter, printed
  in the URL and required for the page and every API call. Requests may pass it
  as `?token=` or the `X-Vela-Token` header.
- Static assets are public only within the loopback server and contain no data.
- HTML is served with a strict CSP: `default-src 'self'`, no inline script, no
  external origins, no `eval`.
- Responses set `X-Content-Type-Options: nosniff` and `X-Frame-Options: DENY`.
- Request headers are capped at 64 KiB and bodies at 2 MiB. The server is
  single-threaded, so there is no cross-request state race.
- The server has no filesystem, process, network or WASI imports. Host calls
  are limited to the deterministic clock/RNG registered by tests.

## UI

- Source editor (WAT) with Load and Restart.
- Export selector and integer argument list.
- Disassembly with current-pc highlight; click an instruction to toggle a
  breakpoint.
- Locals, operand stack, call stack, globals and a memory hex window.
- Timeline slider for rewind, plus Step/Back/Run/To-end controls.
- Breakpoints: instruction, function, memory address, host calls.
- Spec dashboard running the embedded local `.wast` fixtures.
- Run diff panel comparing two argument sets and reporting first divergence.
- SSE trace stream button that consumes `/api/stream` with `EventSource`.

All assets are original, compiled into the binary with `include_str!`. There
are no CDNs, web fonts, images or third-party scripts.

## HTTP API

All routes except `/`, `/app.js`, `/style.css` and `/favicon.ico` require the
token. JSON bodies are parsed by the in-tree JSON parser.

| Method | Route | Purpose |
| --- | --- | --- |
| GET | `/` | Studio HTML |
| GET | `/app.js`, `/style.css` | embedded assets |
| GET | `/api/state?start=N` | full state + memory window at `N` |
| GET | `/api/exports` | function exports of the editor source |
| GET | `/api/spec` | run embedded fixtures |
| GET | `/api/stream` | SSE instruction stream |
| POST | `/api/load` | `{source, export, args}` |
| POST | `/api/action` | `{action: run\|resume\|step\|back\|rewind\|finish\|restart, target?}` |
| POST | `/api/breakpoints` | `add_function`, `toggle_instruction`, `add_memory`, `host_on`, `host_off`, `remove`, `clear` |
| POST | `/api/spec` | run a custom `.wast` source |
| POST | `/api/diff` | `{export, args_a, args_b}` |
| POST | `/api/shutdown` | stop the server |

State responses include step, status, trap, call depth, stack, frames, locals,
globals, memory hex, disassembly, breakpoints, recent trace, exports, trace and
state hashes, checkpoint count and the source itself.

## Server internals

`server.rs` implements HTTP/1.1 manually: request line parsing, header map,
case-insensitive header names, `Content-Length` bodies, percent-decoded query
strings and connection-close responses. SSE writes `event: step` frames with
JSON payloads and one `event: done` summary.

The server handles one connection at a time. The interpreter store uses `Rc`,
so keeping everything on one thread is both simpler and sound; it also removes
any need for locks around the session.

## Playwright test

`studio-tests/test_studio.py` starts the binary with `VELA_NO_OPEN=1`, reads the
printed URL, then:

1. opens the page and waits for `ready`;
2. runs the default module to completion;
3. restarts and toggles an instruction breakpoint by clicking disassembly;
4. runs to the breakpoint, steps forward and back;
5. rewinds the timeline to step 0 with the range input;
6. runs the spec dashboard and checks `41/41`.

Run it with:

```sh
cargo build --release -p vela-studio
python3 studio-tests/test_studio.py
```
