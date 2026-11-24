# Limitations

Vela is a scoped WebAssembly runtime. This file records every known gap so that
no result is overstated.

## Specification coverage

- **The official WebAssembly specification test suite is not vendored in this
  repository.** Vela runs the local `.wast` fixtures in `tests/fixtures/`
  (41 assertions across six scripts) and any script supplied to `vela-spec`.
  No claim of official suite coverage is made.
- Core 1.0 plus multi-value, bulk memory and reference types (`funcref`,
  `externref`) are implemented. Import/export/linking and start functions are
  implemented.
- Out of scope and explicitly rejected by decoder/validator: SIMD, threads and
  atomics, GC, exception handling, tail calls, multiple memories, memory64,
  the component model, WASI, and JIT/native code generation.
- Shared memories and multi-memory modules fail to decode.

## Text format

- The WAT parser supports the subset needed by the examples, fixtures and
  Studio. It does not implement the full grammar: annotations, custom sections,
  GC/SIMD instructions, inline `(data ...)`/`(elem ...)` shorthands beyond the
  supported forms, or quoted identifier edge cases are rejected.
- The name section is ignored; names come from exports or numeric indices.
- WAT pretty-printing uses numeric indices and omits comments and names.
- `.wast` support is a subset: `assert_return`, `assert_trap`,
  `assert_exhaustion`, `assert_invalid`, `assert_malformed`,
  `assert_unlinkable`, NaN assertions, `register`, `invoke`, `get`, text,
  quoted and binary modules. Trap/error message strings are not compared
  verbatim; the assertion checks that the expected *kind* of failure occurs.
- `assert_invalid` distinguishes validation failure from parse/decode failure
  and does not match the specification's exact error text.

## Runtime

- Numeric NaN results use host floating-point behavior for arithmetic, and a
  canonical quiet NaN for `min`/`max`. The specification permits canonical or
  arithmetic NaN results; payload-exact propagation is not promised beyond
  bit-preserving reinterpretation and loads/stores.
- Host imports are only available through the Rust API (`Registry`,
  `define_host`). The Studio does not expose WASI or filesystem/network
  imports, by design.
- `instantiate` runs a start function with the default 10,000,000-step limit.
  A malicious start can consume bounded but significant time before the
  instantiation fails; callers can lower limits in code.
- If instantiation fails after applying segments to *imported* memories/tables,
  the store's newly allocated objects are rolled back but mutations to imported
  objects are not undone. Fresh modules are unaffected.
- Memory growth and initial allocation are capped at 512 MiB per memory by
  default and tables at 1,000,000 entries. Modules declaring larger initial
  sizes fail to instantiate even if the specification would allow them on a
  bigger machine.
- Only `i32` address spaces are supported.
- `ref.func` validation uses the exports/element/global reference set; exotic
  declaration patterns may be reported as invalid.

## Replay and Studio

- Reverse stepping is **checkpoint plus deterministic re-execution**, not an
  inverse interpreter. Work per reverse step is bounded by the checkpoint
  interval (64 by default, 32 in the Studio).
- Checkpoints clone complete memories and tables. Snapshot memory usage grows
  with `steps / interval * state size`; the Studio uses a 300,000-step limit.
- Trace hashing is FNV-1a, not a cryptographic hash.
- The HTTP server is single-threaded. A long SSE stream briefly occupies the
  server; the stream is capped at 2,000 events and 5,000 VM steps.
- The Studio argument editor accepts integer `i32` arguments only. Other
  signatures can be exercised from Rust tests or the `.wast` runner.
- Auto-open uses platform commands (`open`, `xdg-open`, `cmd /C start`); if no
  browser opener exists, the printed URL must be opened manually.
- No CI/CD workflows are provided by design.

## Performance

- The interpreter is a checked, match-based loop without JIT or tiering. It is
  modest by design; benchmark numbers in `docs/benchmarks.md` are machine-local
  and not competitive claims.
- Decoding/validation allocate a `Module` tree; there is no zero-copy view.

## Product scope

Vela is not a database, storage engine, version-control system, CLI product or
terminal UI. `vela-spec` and `vela-bench` are test/harness executables. The
product surface is the embeddable Rust library plus the local Studio.
