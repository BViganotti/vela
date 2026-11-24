# Examples

`examples/` contains 20 hand-written WAT modules. Every result is asserted by
`crates/vela-core/tests/examples.rs`; the numbers below are the expected values,
not descriptions.

| Module | Export | Arguments | Expected |
| --- | --- | --- | --- |
| `factorial.wat` | `fact` | 6 | 720 |
| `fibonacci.wat` | `fib` | 10 | 55 |
| `gcd.wat` | `gcd` | 48, 18 | 6 |
| `memory_copy.wat` | `copy` | — | 0x6c6c6568 (`i32.load` of "hell") |
| `memory_fill.wat` | `fill` | — | 171 |
| `indirect_call.wat` | `pick` | 1 | 2 |
| `traps_div.wat` | `div` | 1, 0 | trap: integer divide by zero |
| `globals_counter.wat` | `counter` | — | 2 |
| `bulk_memory.wat` | `bulk` | — | 190 |
| `multi_value.wat` | `pair` | 7 | (i32 7, i64 700) |
| `loops_sum.wat` | `sum` | 100 | 4950 |
| `bitwise.wat` | `bits` | — | 19 |
| `float_math.wat` | `math` | — | f64 11.0 |
| `conversions.wat` | `convert` | — | i64 4294967298 |
| `select_demo.wat` | `choose` | 10, 20, 1 | 10 |
| `br_table.wat` | `switch` | 2 | 20 |
| `start_function.wat` | `read` | — | 42 |
| `table_grow.wat` | `grow` | — | 2 |
| `nested_calls.wat` | `quad` | 7 | 28 |
| `collatz.wat` | `steps` | 27 | 111 |

See [`examples/README.md`](../examples/README.md) for the per-file guide.

## Coverage

The set exercises: loops, recursion, mutual calls, memory loads/stores, data
segments, `memory.init/copy/fill`, `data.drop`, tables, `call_indirect`,
`table.grow`, mutable/immutable globals, start functions, multi-value returns,
`br_table`, `if/else`, selects, integer and float arithmetic, conversions,
bit operations and a typed trap.

## Running

```sh
cargo test -p vela-core --test examples
```

The test loads each module through the text parser, validates it, instantiates
it with an empty import registry, invokes the named export, and asserts the
exact result values. `every_example_export_resolves` additionally checks that
each named export exists and is a function.

## Studio

The Studio's default module is an additional workload (not counted in the 20):
it writes memory in a loop, updates a global, calls recursive `fib`, and runs
in 424 steps so the timeline and breakpoints have meaningful history.
