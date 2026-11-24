# Performance notes

Vela is correctness-first. This page explains where time goes, which knobs
exist, and how to measure changes without fooling yourself.

## Cost model

Per wasm instruction, the interpreter performs:

1. a fuel check and a variant dispatch;
2. an operand-stack pop/push with a maximum check;
3. type checks on operands (defensive, even though validation proved them);
4. for memory ops, a 64-bit effective-address check against the current length;
5. for calls, a frame allocation and signature/type checks.

Control flow uses precomputed `BodyInfo` (matching `end`/`else` per instruction),
so branches are O(1) index jumps rather than scans.

## Knobs

| Knob | Where | Effect |
| --- | --- | --- |
| `Limits::max_steps` | interpreter/session | upper bound on work; fuel trap |
| `Limits::max_call_depth` | interpreter | host-stack independence; stack-overflow trap |
| `Limits::max_stack` | interpreter | value-stack memory bound |
| `Store::max_memory_bytes` | runtime | memory allocation/growth cap (default 512 MiB) |
| `Store::max_table_entries` | runtime | table allocation/growth cap (default 1M) |
| `Session` checkpoint interval | replay | memory vs rewind cost |
| `Studio` limits | studio | step cap 300k, SSE cap 5k steps/2k events |

Lower caps make hostile inputs cheaper to reject; higher caps allow larger
workloads. There is no configuration file; embedding code sets these fields.

## Checkpoint cost

A checkpoint clones memories, tables, globals, instances, segment state, the
host environment and the execution state. With interval `k`, memory overhead is
roughly `steps / k` snapshots and rewind work is at most `k` re-executed steps.

- Replay tests use interval 1 or 4 for exact checkpoint comparisons.
- The Studio uses 32, which bounds a rewind to 32 instructions while keeping
  snapshot count manageable for 300k-step sessions.

## Tracing cost

Each executed step appends a `TraceEvent` (~50–60 bytes plus allocations for
opcode strings stored as `&'static str`). Long runs are bounded by
`max_steps`; the Studio only renders the last 200 events but the replay crate
retains the full trace for hashing and diffing.

## Measuring

```sh
cargo run --release -p vela-core --bin vela-bench
```

Rules:

- Always use `--release`; debug numbers are not meaningful.
- Warm up once (the benchmark binary does).
- Do not compare decode timings across machines; compare ratios.
- For memory primitives, keep `black_box` so LLVM does not delete the work.
- Report medians or repeated runs if a change is close to noise.

## Known hot spots

- Decoder: LEB128 per byte and `Vec<Instr>` growth; capacity is reserved from
  counts, but very large modules still allocate a full instruction tree.
- Interpreter: the big `match` and per-op type checks dominate. JIT-style
  optimizations are out of scope.
- Replay: cloning memories at checkpoints; delta snapshots are future work.
- Studio: serializing 200 trace events and a 256-byte memory window per state
  poll is cheap compared to the interpreter workload.

## Non-goals

No tiering, no code generation, no SIMD fast paths, no unsafe tricks. Any
optimization must preserve bounds checks, typed traps and deterministic replay
hashes (tests compare hashes, not timing).
