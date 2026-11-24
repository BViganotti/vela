# Error taxonomy

Vela distinguishes failure stages. This matters for `assert_invalid` versus
`assert_malformed`, for embedding, and for honest diagnostics.

## Stages

```
bytes ─ DecodeError ─▶ Module ─ ValidationError ─▶ checked Module
text  ─ TextError  ─▶ Module ─ ValidationError ─▶ checked Module
                                     │
                                     ▼
                            InstantiateError
                                     │
                                     ▼
                         Trap (at runtime)
```

### `DecodeError`

- Byte offset plus a stable message.
- Examples: `magic header not detected`, `unexpected end of input`,
  `integer too large`, `integer representation too long`, `section out of
  order`, `duplicate section`, `malformed section id`, `illegal opcode`,
  `function and code section have inconsistent lengths`.
- Recoverable by treating the input as malformed.

### `TextError`

- Line/column and a message.
- Produced by the lexer, s-expression parser, WAT parser and `.wast` command
  parser.
- Examples: `unterminated block comment`, `unexpected ')'`, `unclosed '('`,
  `unknown instruction`, `unknown local`, `unterminated string`.

### `ValidationError`

- Optional function index and instruction offset, a stable code and a message.
- Codes are coarse (`type mismatch`) while messages are specific (`expected i32
  but found i64`, `unknown label 3`, `data count section required`).
- `assert_invalid` requires this stage, not a parse failure.

### `InstantiateError`

- `Validation(ValidationError)`
- `Import { module, name, message }` — missing import, kind mismatch or type
  mismatch.
- `Limit(&'static str)` — allocation cap would be exceeded.
- `Trap(Trap)` — start function or active segment initialization trapped.
- Failed instantiation rolls back allocations created by that module.

### `Trap`

Runtime failures. `Trap::code()` returns a spec-style string; `Display` adds
detail for host/limit/type mismatches.

| Trap | Typical cause |
| --- | --- |
| `Unreachable` | `unreachable` executed |
| `MemoryOutOfBounds` | load/store/bulk access past memory |
| `TableOutOfBounds` | table get/set/fill/copy/init past bounds |
| `UndefinedElement` | indirect call index out of range |
| `UninitializedElement` | indirect call through null |
| `IndirectCallTypeMismatch` | indirect call signature mismatch |
| `IntegerDivideByZero` | integer div/rem by zero |
| `IntegerOverflow` | `MIN / -1` or truncation out of range |
| `InvalidConversionToInteger` | float-to-int of NaN |
| `StackOverflow` | call-depth cap |
| `OutOfFuel` | step cap |
| `DataSegmentDropped` / `ElemSegmentDropped` | reuse after drop |
| `Host(String)` | host function error |
| `LimitExceeded` | value stack or allocation cap |
| `TypeMismatch` | defensive runtime check or capture type |

### `SessionError` / `ReplayError`

`SessionError` wraps text/validation/instantiation/missing-export/trap failures
during session construction. `ReplayError` covers out-of-range rewind targets,
missing checkpoints and replay traps.

## Guidance

- Never compare only `is_err()` in tests for spec assertions; check the stage.
- Trap messages are not a stable API for exact string matching; use
  `Trap::code()` / `Trap` variants.
- Validation messages are intended for humans and may become more specific
  without changing the stage.
