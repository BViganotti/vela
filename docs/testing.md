# Testing

Every commit in the repository builds and passes the test suite. Vela has no
CI; the suite is designed to run locally and offline.

## Commands

```sh
cargo test                     # debug tests
cargo test --release           # optimized tests
cargo run -p vela-spec -- tests/fixtures
./fuzz/run.sh
cargo run --release -p vela-core --bin vela-bench
python3 studio-tests/test_studio.py
```

## Layers

### Unit tests

- LEB128 and reader limits.
- Lexer strings/comments/positions; s-expression depth and round-trip.
- Value hashing and float bit equality.
- Memory bounds, growth and memmove copy.
- Table growth/type checks.
- Trace hashing and truncation.
- JSON writer/parser and string escapes.
- Text printer round-trips.

### Integration tests

- `tests/decode.rs`: valid module, encode/decode round-trip, truncation at every
  prefix, duplicate/out-of-order sections, unknown opcodes, local limits.
- `tests/validate.rs`: stack typing, control flow, limits, indices, constant
  expressions, segments, exports.
- `tests/runtime.rs`: calls, recursion, loops, memory, tables, indirect calls,
  bulk memory, globals, start, host determinism, fuel and stack limits.
- `tests/numeric.rs`: trap edges, shifts, rotations, rounding, NaN, conversions,
  reinterpretation, stepping.
- `tests/text.rs`: module fields, folded expressions, labels, imports, binary
  round-trip of parsed text, malformed text rejection.
- `tests/spec.rs`: local `.wast` fixtures.
- `tests/examples.rs`: 20 hand-written example modules and their exact results.

### Determinism and time travel

`crates/vela-replay/tests/replay.rs`:

- 100 replays (pure and host-clock) with identical trace/state/memory hashes;
- 500 rewind targets compared against forward state hashes;
- 100 consecutive reverse steps compared against forward;
- instruction/function/memory/host breakpoints;
- run diff after different arguments;
- cross-checks between direct invocation and replay.

### Fuzzing

- 1,000 mutated binary modules + 1,000 random byte strings (`fuzz_modules`);
- 1,000 mutated WAT sources + 500 mutated `.wast` scripts (`fuzz_text`).

### Browser

Playwright drives the real binary: load, run, breakpoint, step, reverse step,
timeline rewind and the spec dashboard, under the production CSP.

## What is deliberately not tested

- The official WebAssembly specification suite is not vendored in this
  repository. Local fixtures are a subset; coverage claims in the final report
  are limited to them.
- No browser automation runs in `cargo test` (requires Python Playwright).
- SIMD/threads/GC/exceptions are out of scope and rejected, not tested as
  features.

## Test philosophy

- Tests assert exact bit patterns for floats where the engine promises them.
- Traps assert the trap kind, not just failure.
- Malformed input tests assert clean rejection instead of "does not crash" only.
- Determinism tests compare hashes, not wall-clock timing.
