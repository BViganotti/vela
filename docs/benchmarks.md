# Benchmarks

Vela prioritizes soundness and inspectability over peak speed. The benchmarks
below exist to catch regressions and to state measured performance honestly.
They are micro-benchmarks, not claims of parity with JIT runtimes.

## How to run

```sh
cargo run --release -p vela-core --bin vela-bench
```

The binary warms up each workload, then reports per-operation time and a rate.
No results are hard-coded. The interpreter stepping benchmark drives
`Machine::step` directly and counts executed instructions.

## Reference run

Environment: macOS (aarch64), Rust 1.99.1, `--release` (thin LTO, opt-level 3),
single run.

| Workload | Iterations | Result |
| --- | ---: | ---: |
| Decode a factorial module (~140 bytes) | 20,000 | ~0.67 µs/op (1.48 M modules/s) |
| Decode + validate | 20,000 | ~1.15 µs/op (871 k modules/s) |
| Instantiate + destroy | 5,000 | ~7.4 µs/op (135 k modules/s) |
| `invoke sum(10_000)` (loop, 10k iterations) | 20 | ~1.4 ms/op (~700 calls/s) |
| Fill 64 KiB through a wasm loop | 100 | ~2.4 ms/op (~415 fills/s) |
| Interpreter stepping, `fib(20)` | 251,743 steps | ~35 M steps/s |
| Raw 64 KiB memory write | 2,000 | ~57 GiB/s |

"Raw 64 KiB memory write" measures `MemoryInst::write` with `black_box` to keep
the optimizer honest; it is a library primitive, not a wasm workload.

## Interpretation

- Decoding and validation are small, allocation-light passes over module bytes.
- Interpretation is a straightforward match-based loop. It is fast enough for
  fixtures, debugging and Studio workloads but performs no JIT compilation.
- Each instruction checks bounds, stack and indices; safety costs are part of
  the measured numbers.

## Methodology notes

- Results vary by machine, thermal state and background load.
- Timings exclude process startup and module parsing unless named.
- The VM limits apply: `Limits::default()` caps steps at 10,000,000; the fib
  benchmark raises the limit for `fib(20)`.
- No numbers are copied from other runtimes or invented.
