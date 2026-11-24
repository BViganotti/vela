# Binary decoder

`vela_core::binary::decode_module` turns a byte slice into a `Module`. The
decoder is the first trust boundary: it must never panic, never allocate
unboundedly and never accept ambiguous encodings.

## Format coverage

- Magic `\0asm`, version 1.
- Sections: type, import, function, table, memory, global, export, start,
  element, code, data, data count, custom.
- Section order and duplicate detection. DataCount is ordered between element
  and code even though its id is 12.
- Module size cap: 64 MiB (`MAX_MODULE_BYTES`).
- Function bodies are decoded into a flat `Vec<Instr>` including `end`/`else`.

## Limits

| Item | Limit |
| --- | ---: |
| Module bytes | 64 MiB |
| Types | 100,000 |
| Imports | 100,000 |
| Functions | 1,000,000 |
| Locals per function | 50,000 |
| Params / results | 10,000 each |
| Structured nesting | 1,000 |
| Instructions per expression | 4,000,000 |
| Element/data segment items | 1,000,000 |
| Memory pages | 65,536 |
| Table entries | 10,000,000 |

Vector counts are also compared with the remaining byte count before reserving
memory, so a count like `0xffffffff` in a tiny section is rejected immediately.

## LEB128

- `u32` uses at most 5 bytes; unused high bits in the final byte must be zero.
- `i32`/`i64` use at most 5/10 bytes with sign-extension checks on the final
  byte. Non-canonical but valid encodings are accepted; out-of-range values are
  rejected with "integer too large" or "integer representation too long".
- `s33` is used for block type indices and has a dedicated sign check.

## Feature policy

Rejected at decode time because they are out of scope:

- shared memories, 64-bit memories and multiple memories;
- SIMD (`0xFD`), atomics (`0xFE`), tail calls, exception handling, GC types;
- unknown opcodes and unknown section ids.

Element segments support all eight flag encodings, including passive and
declarative segments and expression-based initialization. Data segments support
passive and active forms, including an explicit memory index.

## Encoding

`encode_module` is the inverse used for tests, the Studio and fuzz seeds. It
writes sections in canonical order and includes a DataCount section when the
module declares one. Round-trip tests assert that decoding an encoded module
yields the same `Module`.

## Error reporting

`DecodeError` carries a byte offset and a stable message (`unexpected end of
input`, `integer too large`, `section out of order`, `malformed section id`,
`function and code section have inconsistent lengths`, and so on). The offset
is relative to the section reader for nested structures and absolute at the top
level.

## Fuzzing

`cargo test -p vela-core --test fuzz_modules` performs 1,000 deterministic
mutations of a valid module plus 1,000 random byte strings. Every result must be
either a decoded module or a clean `DecodeError`; no panic is acceptable.
