# Changelog

All notable changes to Vela are documented here. The format follows
Keep a Changelog and the project uses semantic versioning.

## [Unreleased]

### Added

- Cargo workspace with `vela-core`, `vela-replay`, `vela-studio` and
  `vela-spec`; zero external dependencies.
- Bounded binary reader and decoder for WebAssembly core 1.0 with LEB128
  validation, section ordering, custom sections, bulk-memory and reference-type
  encodings, and hard size limits.
- Module encoder used for round-trip tests, fuzz seeds and the Studio.
- Specification-style validator with operand/control stacks, unreachable
  polymorphism, constant-expression checks, limits, segment and export rules.
- Interpreter with explicit call frames, typed traps, numeric semantics, linear
  memory growth/bounds checks, tables, reference types, globals, bulk memory,
  host calls and fuel/call-depth/stack limits.
- Instantiation with import type matching, allocation caps, segment
  initialization, start functions and rollback of failed allocations.
- Deterministic `HostEnv` (logical clock, SplitMix64 RNG).
- Text lexer, s-expression parser, WAT module parser (flat and folded), WAT
  printer, and `.wast` assertion parser/runner.
- Local `.wast` fixtures: arithmetic, memory, traps, linking, invalidity,
  floats, tables; 41 assertions.
- `vela-replay`: per-step traces, FNV-1a hashes, checkpoint log, reverse
  stepping, instruction/function/memory/host breakpoints, run diff, replay
  verification.
- Determinism tests: 100 identical replays, 500 rewind checkpoints, sequential
  reverse stepping.
- `vela-studio`: single-threaded loopback HTTP/1.1 + SSE server, token auth,
  CSP, embedded original HTML/CSS/JS UI; disassembly, locals, stack, frames,
  globals, memory hex, timeline, breakpoints, spec dashboard, run diff, SSE
  trace stream.
- Playwright end-to-end Studio test (load, run, breakpoint, step, reverse,
  timeline rewind, spec dashboard).
- 20 hand-written example modules with asserted results.
- Deterministic fuzzing: 1,000 mutated binary modules, 1,000 random byte
  strings, 1,000 mutated WAT sources, 500 mutated `.wast` scripts.
- Core benchmark binary with measured decode/validate/instantiate/execute
  results.
- Documentation for architecture, decoder, validation, interpreter, text,
  replay, Studio API, security, testing, benchmarks, scope and examples.
- MIT license.

### Verified

- `cargo build --release` with zero warnings.
- 253 tests across 36 targets pass in debug and release.
- 41/41 local `.wast` assertions pass.
- 1,000 mutated binary modules + 1,000 random byte strings + 1,000 mutated WAT
  sources + 500 mutated `.wast` scripts complete without panic.
- 100 deterministic replays and 500 rewind checkpoints verified.
- Playwright Studio test passes run, breakpoint, step back, timeline rewind,
  spec dashboard, run diff and SSE stream.
- Benchmarks recorded in `docs/benchmarks.md`; no CI/CD workflows exist.

### Notes

- The official WebAssembly specification suite is not vendored; coverage claims
  are limited to the local fixtures and are reported honestly.
- No CI/CD workflows, no external crates, no runtime network access.
