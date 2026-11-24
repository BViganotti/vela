# Vela

**Vela** is a WebAssembly runtime and time-travel debugger written from scratch
in Rust, using only the standard library. It includes a bounded binary decoder,
a specification-style validator, an interpreter, a WAT/`.wast` subset, a
deterministic replay engine, and a local browser Studio for stepping backward
through execution.

No external crates. No CDN. No network at runtime. No JIT.

## What is inside

- **Decoder** — WebAssembly core 1.0 binary format, bounded LEB128, section
  ordering, limits and malformed-input rejection.
- **Validator** — operand/control stack typing, constant expressions, indices,
  limits, segments and start signatures.
- **Interpreter** — numeric/control/memory/table/reference instructions,
  multi-value, bulk memory, explicit frames, typed traps, fuel and memory caps.
- **Text** — WAT parser (flat and folded), printer, `.wast` assertion runner.
- **Replay** — per-step traces, hashes, checkpoints, reverse stepping,
  instruction/function/memory/host breakpoints, first-divergence diff.
- **Studio** — embedded loopback HTTP/1.1 + SSE server and an original
  HTML/CSS/JS workbench: disassembly, locals, stack, globals, memory hex,
  timeline scrubber, spec dashboard and run diff.

The official WebAssembly spec suite is not vendored. Vela runs its local
fixtures and any `.wast` file you provide. See `LIMITATIONS.md`; no coverage is
claimed beyond what is measured.

## Quick start

```sh
cargo run -p vela-studio
```

The launcher binds an ephemeral port on `127.0.0.1`, prints a URL with a random
auth token, and opens your browser. There is no terminal interaction.

## Library example

```rust
use vela_core::runtime::{instantiate, invoke_export, Limits, Registry, Store, Value};
use vela_core::text::parse_wat;

let wat = r#"
(module
  (func (export "add") (param i32 i32) (result i32)
    (i32.add (local.get 0) (local.get 1))))
"#;
let module = parse_wat(wat).unwrap();
let mut store = Store::new();
let instance = instantiate(&mut store, module, &Registry::new()).unwrap();
let out = invoke_export(
    &mut store,
    instance,
    "add",
    vec![Value::I32(20), Value::I32(22)],
    Limits::default(),
).unwrap();
assert_eq!(out, vec![Value::I32(42)]);
```

Replay uses the same store:

```rust
use vela_replay::{Session, Breakpoint};
use vela_core::runtime::{Limits, Value};

let mut session = Session::from_wat(wat, "add", vec![Value::I32(1), Value::I32(2)], Limits::default()).unwrap();
session.run_to_completion();
let step = session.exec.steps;
session.step_back().unwrap();
assert_eq!(session.exec.steps, step - 1);
```

## Workspace

| Crate | Purpose |
| --- | --- |
| `vela-core` | decoder, validator, interpreter, text, spec runner |
| `vela-replay` | traces, checkpoints, reverse stepping, breakpoints, diff |
| `vela-studio` | loopback HTTP/SSE server, embedded UI, Studio binary |
| `vela-spec` | fixture/spec harness binary (not a product CLI) |

## Commands

```sh
make build          # cargo build --release
make test           # debug + release tests
make spec           # local .wast fixtures (41 assertions)
make fuzz           # deterministic mutation fuzzers
make bench          # core benchmarks
make studio         # launch the Studio
python3 studio-tests/test_studio.py   # Playwright time-travel test
```

## Verified results (0.1.0)

Measured on the development machine (macOS aarch64, Rust 1.99.1):

- `cargo build --release`: zero warnings.
- `cargo test` and `cargo test --release`: 258 tests across 37 targets, all pass.
- Local `.wast` fixtures: 41/41 assertions across 6 files. The official
  WebAssembly suite is not vendored; no broader coverage is claimed.
- Fuzzing: 1,000 mutated binary modules, 1,000 random byte strings, 1,000
  mutated WAT sources and 500 mutated `.wast` scripts, with no panics.
- Determinism: 100 replays (including host clock calls) with identical trace
  and state hashes; 500 rewind checkpoints match forward state; 100 sequential
  reverse steps match forward hashes.
- Studio Playwright test: opens, runs 424 steps, sets an instruction
  breakpoint, steps forward/back, rewinds the timeline, runs the spec
  dashboard, run diff and SSE stream.
- Production source is ~15.4k lines (Rust + embedded UI); tests add ~5.2k.

Details: `FINAL_REPORT.md`, `docs/benchmarks.md`, `docs/testing.md`.

## Documentation

- `docs/architecture.md` — crates, data flow, soundness boundaries
- `docs/decoder.md` — binary format, limits, LEB128, encoding
- `docs/validation.md` — module and body validation
- `docs/interpreter.md` — execution model, traps, numeric semantics
- `docs/text.md` — WAT subset, printer, `.wast` commands
- `docs/replay.md` — traces, checkpoints, reverse stepping, breakpoints, diff
- `docs/studio.md` — security model, UI, HTTP/SSE API
- `docs/security.md` — threat model and resource limits
- `docs/testing.md` — test layers and commands
- `docs/benchmarks.md` — measured performance
- `docs/scope.md` — supported features and non-goals
- `docs/examples.md` — the 20 example modules
- `LIMITATIONS.md` — known gaps, stated plainly
- `FINAL_REPORT.md` — LOC, commits, tests, coverage, benchmarks

## Scope

Implemented: core 1.0, multi-value, bulk memory, reference types, tables,
imports/exports/linking, start functions, deterministic host calls.

Rejected: SIMD, threads/atomics, GC, exceptions, tail calls, multi-memory,
memory64, component model, WASI, JIT.

## License

MIT. See `LICENSE`.
