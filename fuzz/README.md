# Fuzzing

Vela's fuzzers are deterministic and in-tree so they can run in CI-free local
sessions and in `cargo test` without extra tooling or network access.

## What runs

```sh
cargo test -p vela-core --test fuzz_modules
cargo test -p vela-core --test fuzz_text

# or
./fuzz/run.sh
```

- `fuzz_modules`: 1,000 mutations of a valid encoded module (bit flips,
  truncation, insertion, deletion, random bytes) must decode and validate
  without panicking. Well-formed survivors are instantiated when bounded.
- Additional 1,000 random byte strings are fed to the decoder.
- `fuzz_text`: 1,000 mutated WAT sources and 500 mutated `.wast` scripts must
  never panic; malformed scripts always produce a report.

## Guarantees checked

- No panic, index-out-of-bounds, arithmetic overflow or unbounded allocation
  for arbitrary input bytes.
- All parser limits (module size, section counts, locals, nesting, expression
  length, string size) are enforced before allocation.
- Instantiating a mutated module respects the store's memory/table caps.

## Corpus

`fuzz/corpus/seed.wasm` is a valid module with types, memory, table, globals,
data, element segments and a function body. Mutations are derived from it with
a fixed xorshift seed, so failures are reproducible.
