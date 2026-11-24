# Replay and time travel

`vela-replay` turns an execution into an inspectable, deterministic object and
provides reverse stepping without pretending to invert arbitrary instructions.

## Trace

Every forward step records one `TraceEvent` *before* the instruction executes:

```
step, func_addr, offset, op, stack_height, call_depth, stack_hash
```

The trace is the instruction history used by the Studio timeline and by run
diff. `Trace::hash` is FNV-1a over every event field plus the event count.
`Session::trace_hash` and `Session::state_hash` are reported alongside runs.

## Checkpoints

A checkpoint is a full clone of:

- the store: memories, tables, globals, instances, segment drop state and the
  host environment (clock and RNG);
- the execution state: frames, locals, operand stack, control frames, step
  count, results and trap;
- the trace length at that step.

Checkpoints are captured at step 0 and every `interval` steps (default 64; the
Studio uses 32). This is a deliberate memory-for-time trade-off. The caps on
memories and tables bound snapshot size.

## Reverse stepping

`step_back` is `rewind_to(current - 1)`.

`rewind_to(target)`:

1. Reject targets beyond the current step.
2. Restore the newest checkpoint with `step <= target`.
3. Truncate the trace to the checkpoint's recorded length.
4. Re-execute forward with breakpoints ignored, recording events and new
   checkpoints, until the target step.

Because host results are driven by the snapshotted `HostEnv`, and because all
inputs come from the store, re-execution is bit-exact. Reverse stepping is
therefore *checkpoint plus deterministic replay*, not an inverse interpreter.
The work is bounded by the checkpoint interval.

## Breakpoints

- `Function { func_addr }`: stop before the first instruction of a function.
- `Instruction { func_addr, offset }`: stop before one instruction.
- `MemoryAddress(addr)`: stop before a load/store whose effective byte range
  contains `addr`. Store addresses are read from the operand stack.
- `HostCall`: stop before a direct call or indirect call that resolves to a
  host function.

`run` stops before executing the breakpoint instruction. `resume` suppresses
the breakpoint at the current pc once, so pressing Resume advances past it.
`run_to_completion` ignores breakpoints.

## Run diff

`first_divergence(left, right)` compares trace events by function, offset,
opcode, stack height, call depth and stack hash, returning the first difference
with a reason. If one trace is longer, the divergence is reported at the
shorter length. The Studio's Run Diff panel uses this to compare two argument
sets of the same export.

## Determinism tests

`crates/vela-replay/tests/replay.rs` verifies:

- 100 replays of a pure module have identical trace and state hashes;
- 100 replays of a module with host clock calls are identical;
- 500 rewind targets reproduce the forward state and trace-prefix hash;
- 100 sequential `step_back` operations match forward state hashes;
- breakpoints (instruction, function, memory, host) stop at the right pc;
- first divergence is reported after different arguments.

## Hash semantics

FNV-1a is not cryptographic. It is stable, fast and sufficient for regression
and replay identity on a single machine. Float hashes use bit patterns, so NaN
payloads and signed zero are distinguished.
