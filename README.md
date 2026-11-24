# Vela

Vela is a WebAssembly runtime written from scratch in Rust using only the
standard library. It includes:

- a bounded binary decoder and a spec-style validator,
- an interpreter for WebAssembly core 1.0 plus multi-value, bulk memory and
  reference types,
- a WAT subset and a `.wast` assertion runner,
- deterministic recording, replay, reverse stepping and run diffing,
- **Vela Studio**, a local browser workbench with a time-travel debugger.

Vela is not a CLI product and not a database. The Studio is served on
`127.0.0.1` from an embedded HTTP/SSE server with original HTML/CSS/JS compiled
into the binary. There are no third-party crates, no CDN assets, and no network
access at runtime.

## Quick start

```sh
cargo run -p vela-studio
```

The launcher starts a loopback server, prints nothing interactive, and opens
your browser. Use the workspace to load an example, run it, set breakpoints,
step forward and rewind.

## Workspace

| Crate | Purpose |
| --- | --- |
| `vela-core` | decoder, validator, interpreter, text parser, spec runner |
| `vela-replay` | execution traces, checkpoints, reverse stepping, diff |
| `vela-studio` | loopback HTTP/SSE server and embedded UI |
| `vela-spec` | spec/fixture harness (test tool, not the product) |

## Commands

```sh
make build       # cargo build --release
make test        # cargo test and cargo test --release
make spec        # run local .wast fixtures
make fuzz        # 1000 mutated modules must not panic
make bench       # benchmark report
make studio      # launch the Studio
```

## Scope

In scope: core 1.0, multi-value, bulk memory, reference types, table
instructions, deterministic host calls, replay and reverse stepping.

Out of scope: SIMD, threads, GC, exceptions, tail calls, multi-memory, the
component model, WASI, JIT, and network imports. See `LIMITATIONS.md`.

## Documentation

- `docs/architecture.md` — crate and interpreter design
- `docs/replay.md` — recording, checkpoints and reverse stepping
- `docs/studio.md` — HTTP API and UI
- `FINAL_REPORT.md` — measured results and known gaps

## License

MIT. See `LICENSE`.
