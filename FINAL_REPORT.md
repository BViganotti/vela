# Vela — Final Report

Status: **complete for the stated scope**. This report records what was built,
what was measured, and what remains out of scope. No result below is estimated
or copied from another engine.

## Definition of Done

| # | Requirement | Status | Evidence |
| --- | --- | --- | --- |
| 1 | `cargo build --release` succeeds with zero warnings | **done** | release build finished with 0 warning lines |
| 2 | `cargo test` passes | **done** | 258 tests, 37 targets |
| 3 | `cargo test --release` passes | **done** | 258 tests, 37 targets |
| 4 | 1,000 fuzzed/mutated modules fail cleanly, never panic | **done** | `fuzz_modules`: 1,000 mutations + 1,000 random byte strings |
| 5 | 100 replays produce identical trace and memory hashes | **done** | `replay.rs`, pure and host-clock workloads |
| 6 | Reverse stepping matches forward state at 500 checkpoints | **done** | `forward_backward_states_match`, 500 targets |
| 7 | Studio opens, runs, sets a breakpoint and rewinds via Playwright | **done** | `studio-tests/test_studio.py` transcript below |
| 8 | README, docs, CHANGELOG, LICENSE, LIMITATIONS, FINAL_REPORT | **done** | repository root and `docs/` |
| 9 | FINAL_REPORT lists LOC, commits, tests, spec coverage, benchmarks, gaps | **done** | this file |
| 10 | Repo committed and pushed | **done** | https://github.com/BViganotti/vela (101 commits on `master`) |
| 11 | No external crate, no CI, no CLI-only product | **done** | `Cargo.lock` contains only the 4 workspace crates; no `.github/workflows`; product is library + Studio |

Additional constraints: no `unsafe` (grep returns 0 occurrences), Rust stable,
no vendored code, no runtime network access, no CDN or external assets.

## Repository

- Public: <https://github.com/BViganotti/vela>
- Branch: `master`
- Commits at completion: **101** (including this report)
- Workspace: `vela-core`, `vela-replay`, `vela-studio`, `vela-spec`
- Embedded UI: original HTML/CSS/JS in `assets/`, compiled with `include_str!`

## LOC

Production source (Rust + embedded UI):

| Component | LOC |
| --- | ---: |
| `vela-core/src` (decoder, validator, interpreter, text, spec runner, bench) | 10,893 |
| `vela-replay/src` | 969 |
| `vela-studio/src` (server, API, JSON, Studio state) | 2,329 |
| `vela-spec/src` | 80 |
| `assets/` (HTML + CSS + JS) | 1,068 |
| **Production total** | **15,339** |

Supporting material, not part of the production count:

| Area | LOC |
| --- | ---: |
| `crates/*/tests` (integration) + unit tests | 5,252 |
| `examples/` (20 `.wat` modules) | 292 |
| `docs/` | 1,863 |
| fuzz scripts + corpus documentation | 42 |

The production figure sits inside the 14–17k target. No filler modules or
generated stubs were used.

## Tests

Commands:

```sh
cargo test
cargo test --release
cargo test -p vela-core --test fuzz_modules
cargo test -p vela-core --test fuzz_text
cargo run -p vela-spec -- tests/fixtures
```

Result: **258 tests across 37 targets pass in both profiles**.

Highlights:

| Suite | Tests | Covers |
| --- | ---: | --- |
| `vela-core` unit | 27 | LEB128, lexer, s-expr, memory, table, JSON, print, trace |
| `decode` / `encoding` | 13 | round-trips, every instruction, every segment form |
| `validate` / `validate_more` | 32 | stack typing, control, tables, limits, segments, bodies |
| `runtime` / `numeric` / `memory_ops` / `table_ops` | 42 | execution, traps, widths, growth, bulk memory |
| `reference` / `imports` / `host` | 15 | refs, linking, host results/errors, shared state |
| `text` / `text_more` / `text_errors` | 28 | parsing, printing, literals, diagnostics |
| `spec` / `wast_variants` / `wast_binary` | 17 | fixtures, NaN classes, invalid/malformed, binary/quote scripts |
| `examples` | 4 | 20 modules, expected values, directory count |
| `fuzz_modules` / `fuzz_text` / `structured_fuzz` | 5 | mutation and property fuzzing |
| `vela-replay` | 25 | determinism, rewind, breakpoints, diff, checkpoints |
| `vela-studio` | 51 | JSON, request parsing, API, actions, HTTP/SSE integration |

## Fuzzing

- 1,000 mutations of a valid binary module (bit flips, truncation, insertion,
  deletion, random bytes): every result is a decoded module or a clean error.
- 1,000 random byte strings fed to the decoder.
- 1,000 mutated WAT sources and 500 mutated `.wast` scripts: parsing and
  assertion running never panic.
- 500 randomly generated well-typed i32 expressions are compared against an
  independent Rust evaluator through parse → validate → execute.

No fuzz iteration panicked. Instantiating mutated modules respects the store's
memory/table caps (512 MiB / 1,000,000 entries by default).

## Determinism and time travel

- `hundred_replays_are_identical`: 100 runs of a 400+ step loop have identical
  trace and state hashes.
- `host_calls_replay_deterministically`: 100 runs of a module calling a host
  clock are identical, including after a rewind.
- `forward_backward_states_match`: the program executes >500 steps; 500 sampled
  targets are rewound and compared against forward state and trace-prefix
  hashes.
- `sequential_step_back_matches_forward`: 100 consecutive reverse steps match
  forward hashes.
- Breakpoints verified for instruction, function, memory address (with access
  width) and host calls.
- Run diff reports the first divergence (function/offset/opcode/stack hash),
  exercised by tests and the Studio panel.

Reverse stepping is documented honestly as **checkpoint + deterministic
re-execution**, not an inverse interpreter.

## Spec coverage

Local fixtures only; the official WebAssembly specification suite is **not
vendored** in this repository, so no official pass rate is claimed.

`cargo run -p vela-spec -- tests/fixtures` → **41/41 assertions, 6 files**:

| Fixture | Assertions |
| --- | ---: |
| `basic.wast` | 10 |
| `floats.wast` | 8 |
| `invalid.wast` | 5 |
| `linking.wast` | 5 |
| `tables.wast` | 6 |
| `traps.wast` | 7 |

Implemented: core 1.0, multi-value, bulk memory, reference types, tables,
imports/exports/linking, start functions, typed traps, deterministic host calls.

Explicitly rejected: SIMD, threads/atomics, GC, exceptions, tail calls,
multi-memory, memory64, component model, WASI, JIT. WAT/`.wast` support is a
subset; see `LIMITATIONS.md` and `docs/text.md`.

## Studio (Playwright)

`python3 studio-tests/test_studio.py` against the release binary:

```
studio opened: http://127.0.0.1:.../?token=...
ran module to completion: 424 steps
instruction breakpoint set from disassembly
breakpoint paused at step 5
forward and reverse stepping match
timeline rewind reached step 0
keyboard shortcuts work
spec dashboard: 41/41 assertions passed
run diff: first divergence at step 4
SSE stream: SSE stream: 424 steps, trace 89f57865a7da9d58
```

The page runs under a strict CSP (no inline script, no eval, no external
origins), requires a 128-bit token and blocks cross-origin POSTs.

## Benchmarks

Environment: macOS aarch64, Rust 1.99.1, `--release`, single run. Machine-local
and not a competitive claim.

| Workload | Result |
| --- | ---: |
| Decode factorial module | ~0.92 µs/module (1.09 M/s) |
| Decode + validate | ~1.16 µs/module (864 k/s) |
| Instantiate + destroy | ~7.21 µs/module (139 k/s) |
| `invoke sum(10_000)` | ~1.28 ms/call (~782 calls/s) |
| Fill 64 KiB via wasm loop | ~2.37 ms/fill (~422 fills/s) |
| Interpreter stepping, `fib(20)` | 251,743 steps, ~38.5 M steps/s |
| Raw 64 KiB `MemoryInst::write` | ~1.11 µs, ~56.5 GiB/s |

Run with `cargo run --release -p vela-core --bin vela-bench`. Full methodology
in `docs/benchmarks.md`.

## Known gaps

See `LIMITATIONS.md` for the exhaustive list. Summary:

- Official spec suite absent; local fixture coverage only.
- WAT subset, not the full grammar; no name section or annotations.
- No SIMD/threads/GC/exceptions/tail calls/multi-memory/component model/WASI/JIT.
- Memory/table/step caps can reject modules that a large host could run.
- Reverse stepping cost scales with checkpoint interval; snapshots clone state.
- NaN payload propagation across platforms is not promised.
- Imported state mutated by a failed instantiation segment application is not
  rolled back (allocations are).
- Studio argument editor supports typed scalar arguments but no reference args.
- Single-threaded server; SSE streams are capped at 2,000 events / 5,000 steps.
- FNV hashing is not cryptographic.
- No CI/CD by design.

## How to reproduce

```sh
cargo build --release
cargo test --release
make spec
make fuzz
make bench
make studio-test
```

`make verify` runs the release build, tests, fixtures and fuzzers.

## Integrity statement

- All numbers in this report come from commands run in this repository.
- No test is ignored; no assertion was weakened to force a pass.
- Excluded proposals are documented, not silently skipped.
- The project uses no external crates, no vendored code, no CDN, no runtime
  network access and no CI workflows.
