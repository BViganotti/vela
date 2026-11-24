# Example modules

Twenty hand-written WAT modules, each with an asserted result in
`crates/vela-core/tests/examples.rs`. They are intentionally small but cover
the runtime's main surfaces.

| File | Demonstrates |
| --- | --- |
| `factorial.wat` | loop, `br_if`, accumulating local |
| `fibonacci.wat` | iterative state juggling |
| `gcd.wat` | `rem_u`, loop with branch |
| `memory_copy.wat` | byte loads/stores, data segment |
| `memory_fill.wat` | `memory.fill`, narrow load |
| `indirect_call.wat` | table plus `call_indirect` |
| `traps_div.wat` | typed `IntegerDivideByZero` trap |
| `globals_counter.wat` | mutable global |
| `bulk_memory.wat` | `memory.init/copy/fill`, `data.drop` |
| `multi_value.wat` | multi-value results |
| `loops_sum.wat` | arithmetic loop |
| `bitwise.wat` | popcount and rotate |
| `float_math.wat` | `f64.sqrt`, ties-to-even nearest |
| `conversions.wat` | float→int and unsigned extension |
| `select_demo.wat` | `select` conditional |
| `br_table.wat` | nested blocks and `br_table` |
| `start_function.wat` | `(start ...)` writes memory |
| `table_grow.wat` | `table.grow`/`table.size` with null refs |
| `nested_calls.wat` | function calls |
| `collatz.wat` | if/else inside a loop |

## Run

```sh
cargo test -p vela-core --test examples
```

The test parses each module from text, validates, instantiates with an empty
import registry, invokes the named export and compares bit-exact values. The
trap example asserts the trap kind. A final test asserts that the directory
contains exactly twenty `.wat` modules.

## Use them in the Studio

Paste a module into the editor and choose its export. Modules with imports
(only the linking fixture uses them) cannot run in the Studio because the
editor has no import registry; they are exercised from Rust and `.wast`.
