# Architecture

Vela is a Cargo workspace with four crates. It uses only the Rust standard
library: no third-party crates, no vendored code, no procedural macros and no
build scripts.

```
vela/
  crates/vela-core      decoder, validator, interpreter, text, spec runner
  crates/vela-replay    traces, checkpoints, reverse stepping, run diff
  crates/vela-studio    loopback HTTP/SSE server and embedded UI
  crates/vela-spec      test harness binary (not the product)
  assets/               original HTML/CSS/JS compiled into vela-studio
  examples/             20 hand-written .wat modules with expected results
  tests/fixtures/       local .wast suites
  fuzz/                 deterministic mutation fuzzers and corpus seed
```

## Data flow

```
bytes ──decode_module──▶ Module ──validate_module──▶ checked Module
                                                     │
text ──parse_wat────────▶ Module ────────────────────┤
                                                     ▼
                                              instantiate()
                                                     │
                                        Store + Instance (export map)
                                                     │
                               Machine::step / invoke (interpreter)
                                                     │
                     vela-replay Session (trace, checkpoints, breakpoints)
                                                     │
                                     vela-studio API (JSON + SSE)
```

The `Module` is a plain owned tree of instructions; it is shared through
`Rc<Module>` inside instantiated instances. The store owns all mutable state:
functions, tables, memories, globals, instances, segments and the deterministic
host environment.

## Soundness boundaries

1. **Decoder**: bounded reader, checked LEB128, size limits for every section
   and vector, maximum nesting, maximum locals, explicit rejection of
   unsupported features (multi-memory, shared memories).
2. **Validator**: the specification's operand/control stack algorithm with
   unknown types after `unreachable`. Indices, types, limits, constant
   expressions, exports and start signatures are checked before execution.
3. **Runtime**: every memory access is range-checked with 64-bit arithmetic
   before conversion; every table access is checked; every label and local
   index was validated but is still bounds-checked; all allocation goes through
   caps (`Store::max_memory_bytes`, `Store::max_table_entries`).
4. **Interpreter**: a step is exactly one instruction. Stack growth, call depth
   and fuel are hard limits that produce typed traps. Malformed state cannot
   panic; failures become `Trap` values.
5. **Replay**: a checkpoint clones the entire mutable store (memories, tables,
   globals, segments, host clock and RNG state) plus the execution state.
   Reverse stepping restores a snapshot and deterministically re-executes.

## Interpreter model

Execution uses an explicit frame stack, not native recursion:

- `Frame { module, defined_idx, locals, ip, stack_base, ctrl_base, ... }`
- `Ctrl { kind, start_ip, end_ip, stack_height, arity, end_arity }`
- `ExecState { frames, stack, ctrls, steps, finished, results, trap }`

`BodyInfo` precomputes the matching `end` (and `else`) for every structured
instruction, so branches are O(1) jumps. Function calls push frames; `return`
and the function's final `end` pop them and copy results to the caller's stack
base. Calls have no native recursion, so deep wasm recursion cannot overflow
the host stack; it hits `Trap::StackOverflow` at the configured depth.

## Why no crates

The project is intentionally self-contained. HTTP, JSON, SSE, LEB128, WAT
parsing and the UI are implemented here. This keeps the dependency graph empty,
makes offline builds trivial and forces explicit limits instead of inheriting
unsafe defaults from generic parsers.

## Determinism

- Values are compared and hashed bitwise (floats by bit pattern).
- The host environment exposes a logical clock and a SplitMix64 RNG, both part
  of the snapshot.
- Memories and tables are deterministic vectors; segment drops are explicit.
- Replay hashes cover trace events, interpreter state and store state.

See `docs/replay.md` for the time-travel model and `docs/studio.md` for the API.
