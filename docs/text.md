# Text format and `.wast`

`vela_core::text` implements the WAT subset needed by the examples, local spec
fixtures and Studio editor, plus a `.wast` assertion runner.

## Lexer

- Parentheses, atoms and quoted strings.
- Byte strings with `\n`, `\t`, `\r`, `\"`, `\\`, `\'`, `\XX` hex bytes and
  `\u{...}` Unicode escapes. Raw newlines/control characters are rejected.
- `;;` line comments and nested `(; ... ;)` block comments with a depth limit.
- Line/column tracking for diagnostics and a 16 MiB input cap.

## S-expression parser

The tree is `Atom`, `Str(bytes)` or `List`. Nesting is capped at 1,000 levels.
Unbalanced or trailing parentheses produce a `TextError` with position.

## Module parser

Supported fields:

```
(type ...) (import ...) (func ...) (table ...) (memory ...) (global ...)
(export ...) (start ...) (elem ...) (data ...)
```

Two passes are used: the first registers type names and every name in each
index space, the second builds bodies and resolves names. Imports must appear
before definitions of the same kind; a violation is a parse error.

Supported syntax highlights:

- named types, functions, tables, memories, globals, params and locals;
- inline `(import "m" "n")` and `(export "name")` on definitions;
- folded expressions such as `(i32.add (local.get $x) (i32.const 1))`;
- flat structured control (`block $l ... end`, `if ... else ... end`);
- folded `if` with `(then ...)` and `(else ...)`;
- labels referenced by name or depth;
- block result annotations, `(type $t)` and implicit multi-value block types;
- integer hex/unsigned literals and float hex/NaN payload literals;
- active/passive/declarative element segments;
- active/passive data segments with string concatenation;
- all instructions the interpreter implements.

Out of scope (explicit parse errors): inline data/elem shorthands beyond the
supported forms, GC/SIMD/threads, annotations, custom sections in text, and
identifier-only modules without `(module ...)`.

## Printing

`module_to_wat` renders any `Module` back to canonical text with numeric names.
It is used by tests and can be used to inspect decoded binary modules. Function
bodies omit the implicit final `end`; blocks are indented two spaces per level.

## `.wast`

Commands:

- `(module ...)`, `(module binary "...")`, `(module quote "...")`, optional
  `$name`;
- `(register "name" $module?)`;
- `(invoke ...)`, `(get ...)`;
- `(assert_return ...)`, `(assert_trap ...)`, `(assert_exhaustion ...)`;
- `(assert_invalid ...)`, `(assert_malformed ...)`, `(assert_unlinkable ...)`;
- `(assert_return_canonical_nan ...)`, `(assert_return_arithmetic_nan ...)`.

Assertions are honest: `assert_invalid` requires a validation failure,
`assert_malformed` requires a parse/decode failure, traps must actually occur,
and NaN expectations check payload classes. Failures are reported with the
command's source line. Local fixtures live in `tests/fixtures/` and are also
embedded in the Studio's spec dashboard.

## `.wast` subset limitations

The official WebAssembly test suite is not vendored. Vela runs its local
fixtures and any `.wast` file supplied on the command line. Unsupported
commands or proposals are reported as failures rather than silently skipped.
See `LIMITATIONS.md`.

## Running

```sh
cargo run -p vela-spec -- tests/fixtures
cargo test -p vela-core --test spec
```
