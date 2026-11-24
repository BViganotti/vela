# Verification log

This is a reproducible record of the checks used for the 0.1.0 completion. It is
not a CI dashboard; it is the transcript of local runs.

Environment: macOS (aarch64), Rust 1.99.1 stable, release profile with thin LTO.

## Build

```sh
$ cargo build --release
    Finished `release` profile [optimized] target(s)
$ cargo build --release 2>&1 | grep -c warning
0
```

## Tests

```sh
$ cargo test
... 36 targets ...
$ cargo test --release
... 36 targets ...
$ cargo test --release 2>&1 | grep "test result" | awk '{s+=$4} END {print s}'
258
```

Both profiles pass with zero failures and zero ignored tests.

## Fixtures

```sh
$ cargo run -q -p vela-spec
ok   tests/fixtures/basic.wast: 10/10 assertions passed
ok   tests/fixtures/floats.wast: 8/8 assertions passed
ok   tests/fixtures/invalid.wast: 5/5 assertions passed
ok   tests/fixtures/linking.wast: 5/5 assertions passed
ok   tests/fixtures/tables.wast: 6/6 assertions passed
ok   tests/fixtures/traps.wast: 7/7 assertions passed

41/41 assertions passed in 6 file(s), 0 file(s) failed
```

The official WebAssembly suite is not present; these are local fixtures.

## Fuzzing

```sh
$ ./fuzz/run.sh
test result: ok. 2 passed   # 1,000 mutated modules + 1,000 random byte strings
test result: ok. 2 passed   # 1,000 mutated WAT + 500 mutated .wast scripts
```

In addition, `structured_fuzz` compares 500 random well-typed expressions
against an independent Rust evaluator.

## Determinism

`cargo test -p vela-replay --test replay`:

- `hundred_replays_are_identical`
- `host_calls_replay_deterministically`
- `forward_backward_states_match` (500 checkpoints)
- `sequential_step_back_matches_forward` (100 reverse steps)
- breakpoint tests for instruction, function, memory and host calls
- `first_divergence_is_reported`

`cargo test -p vela-replay --test primitives` adds checkpoint ordering, trace
statistics, memory-access widths and trap trace consistency.

## Studio (Playwright)

```sh
$ cargo build --release -p vela-studio
$ python3 studio-tests/test_studio.py
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

## Benchmarks

```sh
$ cargo run --release -p vela-core --bin vela-bench
decode factorial module            20000 iters        0.92 us/op   1086957 modules/s
validate factorial module          20000 iters        1.16 us/op    864304 modules/s
instantiate + destroy               5000 iters        7.21 us/op    138696 modules/s
invoke sum(10_000)                    20 iters     1278.87 us/op        782 calls/s
fill 64 KiB via wasm loop            100 iters     2371.79 us/op        422 fills/s
interpreter stepping              251743 steps  fib(20)=[I32(6765)]  38534058 steps/s
memory write 64 KiB                 2000 iters        1.11 us/op     56550.4 MiB/s
```

Machine-local numbers; see `docs/benchmarks.md` for methodology.

## Repository checks

```sh
$ grep -rn "unsafe" crates --include='*.rs'   # no output
$ ls .github                                  # absent
$ cat Cargo.lock                              # only vela-core, vela-replay,
                                              # vela-studio, vela-spec
```
