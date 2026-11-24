# Determinism

Vela's replay and time-travel features depend on a simple contract:

> Given the same module, the same arguments and the same initial host seed, an
> execution produces the same trace events, the same final store and the same
> hashes, every time.

## Sources of state

The entire mutable universe of an execution is:

- `Store`: functions (immutable), memories, tables, globals, instances,
  element/data segment contents and the host environment.
- `ExecState`: call frames, locals, operand stack, control frames, step count,
  results and trap.
- The input module (`Rc<Module>`, immutable) and host functions (shared, but
  their observable effects go through `HostEnv`).

Checkpoints clone both `Store` and `ExecState`. Host functions and modules are
shared by reference counting, but they are not mutated.

## Determinism choices

- Memory is a zero-initialized byte vector; growth zero-fills.
- Tables are vectors of reference values; null is explicit.
- Element/data segment drops are explicit `Option::None` in the instance.
- Export maps are `BTreeMap`, so key iteration is sorted.
- Float values are compared and hashed by bit pattern; `-0.0 != 0.0` for hashing
  and for spec equality, as required for exact assertions.
- Arithmetic uses IEEE semantics from the host FPU. The same binary on the same
  platform produces the same bits; NaN payload propagation across platforms is
  not promised (see `LIMITATIONS.md`).
- The host clock is a counter incremented per observation, not wall time.
- The host RNG is SplitMix64 with an explicit seed.
- The interpreter is single-threaded and all stores are `Rc`-based, so no data
  races are possible.

## Hashes

- `Trace::hash`: FNV-1a over step, function address, instruction offset, opcode
  bytes, stack height, call depth and a hash of the operand stack *before* the
  instruction.
- `Session::state_hash`: step, finished/trap, frames (address, pc, locals),
  operand stack, control frames, results and `Store::state_hash`.
- `Store::state_hash`: memories, tables, globals, segment state and host clock
  and call count.

FNV is used for compact comparisons. It is not collision-resistant; an
adversary could construct collisions. This is acceptable for a local debugger
and is documented rather than hidden.

## What is not deterministic

- Wall-clock time, process scheduling, addresses and object identity.
- Floating-point results around NaN payload propagation across different CPUs
  or compilers.
- Host functions that ignore `HostEnv` and read ambient OS state. The engine
  cannot prevent that; the provided helpers are deterministic.
- Trace hashes if the module itself changes or the host registry differs.

## Verification

Tests in `crates/vela-replay/tests/replay.rs`:

- 100 identical replays of a pure loop;
- 100 identical replays of a module that calls a host clock;
- 500 rewind targets checked against forward state and trace hashes;
- 100 sequential reverse steps;
- replay after rewind including host calls.

`crates/vela-replay/tests/primitives.rs` additionally checks checkpoint
ordering, trace statistics and memory breakpoint ranges.

## Replay after rewind

Rewinding restores a snapshot, then re-executes forward. This is deterministic
because host state is restored *before* host calls are replayed. The trace
prefix is truncated and rebuilt, so the trace always corresponds to the
current step count.
