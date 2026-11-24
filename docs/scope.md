# Scope

## Product

Vela is two things:

1. an embeddable Rust library (`vela-core` + `vela-replay`) for decoding,
   validating, executing and inspecting WebAssembly;
2. a local browser workbench (`vela-studio`) with deterministic time travel.

It is not a CLI product, not a terminal UI, not a database, not a storage
engine, and has nothing to do with version control, branches or repositories.
`vela-spec` and `vela-bench` are harness binaries for tests and measurements.

## Supported WebAssembly

| Area | Status |
| --- | --- |
| Binary format core 1.0 | yes |
| Validation (stack typing, limits, const exprs) | yes |
| Numeric i32/i64/f32/f64 instructions | yes |
| Control flow, calls, locals, globals | yes |
| Linear memory, growth, bounds checks | yes |
| Tables, funcref/externref, `call_indirect` | yes |
| Imports/exports, start, data/element segments | yes |
| Multi-value block/func signatures | yes |
| Bulk memory (`memory.init/copy/fill`, `data.drop`) | yes |
| Reference instructions (`ref.null/is_null/func`, table ops) | yes |
| Multi-memory | no (rejected) |
| SIMD, threads/atomics, GC, exceptions, tail calls | no (rejected) |
| WASI, filesystem/network imports | no |
| JIT / native code generation | no |

## Host interface

The Rust API exposes a `Store`, `Instance`, typed `Value`s, `Registry` and host
functions. Host functions observe a deterministic `HostEnv` (logical clock and
SplitMix64 RNG). There is no ambient OS access in modules.

## Determinism

- Identical module + arguments + host seed produce identical traces, state
  hashes and memory hashes.
- Replay snapshots include host-observable state.
- Time travel is snapshot + re-execution.

## Deliberate non-goals

- Competing with JIT runtimes on throughput.
- Implementing the full WAT grammar or the component model.
- Vendoring the official spec suite or reporting hypothetical coverage.
- Network or terminal interaction in the product.
- CI configuration.
