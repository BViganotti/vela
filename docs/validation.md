# Validation

Validation is mandatory before instantiation. `validate_module` checks the
entire module and returns a `ValidationError` with a stable code, a function
index (when applicable) and an instruction offset.

## Module-level checks

- Type indices referenced by imports and functions exist.
- Function signatures index the type section.
- At most one memory (multi-memory is out of scope); memory and table limits are
  within hard maxima and `min <= max`.
- Table element types are reference types; imports have valid global types.
- Global initializers are constant expressions of the declared type.
- Active element offsets are `i32` constants; the table exists and its element
  type matches the segment.
- Active data offsets are `i32` constants; the memory exists.
- DataCount, when present, equals the number of data segments. `memory.init`
  and `data.drop` require a DataCount section.
- Export names are unique; each export index is in range.
- A start function exists and has type `[] -> []`.
- `ref.func` indices are in the module's reference set (exports, element and
  global initializers), matching the 2.0 validation rule.

## Function bodies

The operand/control stack algorithm from the specification appendix is used:

- `vals: Vec<Option<ValType>>` where `None` is the unknown type produced in
  unreachable code.
- `ctrl: Vec<CtrlFrame { kind, start_types, end_types, height, unreachable }>`.

Operations:

- `pop_val` checks the current frame height; at the height, an unreachable
  frame yields `Unknown` instead of underflowing.
- Entering a block pops its parameters and pushes a control frame whose height
  is below the re-pushed parameters (so branches can restore them).
- `br`/`br_if`/`br_table` use the label's arity: loop labels use start types,
  block/if labels use end types.
- `br_table` requires all targets to have identical types and arity.
- `return` uses the function's result types and marks the rest unreachable.
- `if` without `else` requires start types equal to end types.
- `select` requires numeric operands; `select (result t)` requires exactly one
  type in this profile.
- `call` and `call_indirect` check parameter and result types; indirect calls
  require a `funcref` table and a valid type index.
- Memory loads/stores check memory existence and natural alignment
  (`2^align <= byte width`).
- Table instructions check table and segment indices and type compatibility.
- Bulk-memory instructions require a memory, segment indices and DataCount.
- Reference instructions check reference types and the `refs` set.
- Numeric instructions use a central signature table, preventing drift between
  decoder, interpreter and validator.

## Constant expressions

Only `i32.const`, `i64.const`, `f32.const`, `f64.const`, `ref.null`, `ref.func`,
`global.get` and `end` are accepted. `global.get` may reference imported
immutable globals only. The expression must leave exactly one value of the
expected type.

## Why this order

Instantiation consumes a `Module` and trusts it. Keeping all index and type
checks in one pass before any allocation means a malformed module cannot create
partially initialized runtime state. Instantiation still performs import type
matching and segment bounds checks, which depend on the store.

## Tests

`crates/vela-core/tests/validate.rs` covers stack typing, unreachable
polymorphism, control-flow mismatches, memory alignment, immutable globals,
duplicate exports, start signatures, constant expressions, segment indices,
`ref.func` declaration and multi-memory rejection.
