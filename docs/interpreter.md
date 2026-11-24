# Interpreter

Vela is an interpreter only. It does not generate native code, so every wasm
instruction is checked and executed in a Rust `match`.

## Values

```
Value = I32(i32) | I64(i64) | F32(f32) | F64(f64)
      | FuncRef(Option<u32>) | ExternRef(Option<u32>)
```

Reference values hold store addresses. `None` is null. Equality and hashing use
bit patterns for floats, which makes NaN payloads observable to replay. Values
carry no tags beyond the enum; types are enforced by validation and defensive
runtime checks.

## Execution state

```
ExecState {
  frames: Vec<Frame>,   // explicit call stack
  stack:  Vec<Value>,   // operand stack
  ctrls:  Vec<Ctrl>,    // structured control frames
  steps:  u64,
  finished: bool,
  results: Vec<Value>,
  trap: Option<Trap>,
}
```

Each function call pushes a `Frame`. Locals are parameters followed by declared
locals initialized to their type's zero/null value. A function-level `Ctrl`
frame shares return/branch handling with blocks.

`Ctrl` stores `stack_height`, branch arity and end arity. For a loop with
parameters, the branch arity is the parameter count while normal completion
uses the result count; keeping these separate avoids the classic loop-label
bug.

## Step semantics

`Machine::step` executes exactly one instruction:

1. Check finished, trap and fuel.
2. Capture the current program counter (used by replay).
3. Advance `ip`.
4. Execute; calls may push frames, returns may pop them.
5. Record a typed trap on failure.

Because a step never recurses natively, a paused machine resumes at the next
instruction without re-running the call.

## Traps

Typed traps: `Unreachable`, `MemoryOutOfBounds`, `TableOutOfBounds`,
`IndirectCallTypeMismatch`, `UninitializedElement`, `UndefinedElement`,
`IntegerDivideByZero`, `IntegerOverflow`, `InvalidConversionToInteger`,
`StackOverflow`, `OutOfFuel`, `DataSegmentDropped`, `ElemSegmentDropped`,
`Host`, `LimitExceeded`, `TypeMismatch`.

Integer division, truncation, unreachable, null indirect calls, out-of-bounds
memory/table access and exhausted resources are all observable as typed traps.
No host panic is used for control flow.

## Numeric semantics

- Integer arithmetic wraps; division by zero traps; `MIN / -1` traps; `MIN % -1`
  is zero.
- Shift counts are masked (mod 32/64); rotations follow the same rule.
- Float `min`/`max` propagate a canonical quiet NaN and handle signed zero.
- `nearest` uses ties-to-even.
- Float-to-int truncation traps on NaN or out-of-range values; saturating
  variants clamp instead.
- Reinterpret instructions preserve bits exactly.

## Memory and tables

Memories are `Vec<u8>` with a page size of 65,536. Address computation uses
64-bit arithmetic with `addr + offset + width` before conversion, so wrapping
cannot fake an in-bounds access. Growth is capped by both the declared maximum
and the store allocation cap; failed growth returns `-1` rather than trapping.

Tables store reference values with element-type checks. `call_indirect`
distinguishes out-of-range slots (`UndefinedElement`), null slots
(`UninitializedElement`) and function-type mismatches.

## Host calls

Host functions receive `&mut HostEnv` and an argument slice. Their results are
type-checked before being pushed. `HostEnv` provides a logical millisecond clock
and a SplitMix64 RNG; both are part of store snapshots, making host-observable
execution deterministic.

## Limits

```
max_steps      default 10,000,000
max_call_depth default 1,024
max_stack      default 1,000,000 values
max_memory     default 512 MiB per memory
max_table      default 1,000,000 entries
```

Limits are configuration, not global constants, so tests and the Studio can
choose smaller bounds.

## Tests

`tests/runtime.rs` covers calls, recursion, loops, `br_table`, memory, tables,
indirect calls, bulk memory, globals, start functions, host determinism and
fuel/stack limits. `tests/numeric.rs` covers arithmetic, traps, rounding,
conversions and bit reinterpretation. `tests/examples.rs` runs 20 modules.
