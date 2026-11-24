# Security and robustness

Vela treats module bytes, WAT text and Studio requests as untrusted input.
This document lists the boundaries and the checks that enforce them.

## Threat model

In scope:

- malicious WebAssembly modules that try to escape bounds, exhaust memory,
  trigger panics, forge indices, or trap confusingly;
- malicious or corrupt WAT/`.wast` text;
- a local web page or process trying to call the Studio API without the token;
- a malicious module in the Studio editor.

Out of scope:

- OS-level compromise, side channels, speculative execution attacks;
- hostile browser extensions;
- network attackers: the server only binds loopback.

## Memory safety

- `#![forbid(unsafe_code)]` is set at the workspace level. There is no `unsafe`
  block in the project.
- All indexing goes through `get`/`get_mut` or checked slices. Parser counts are
  validated against remaining bytes before allocation.
- Every memory access uses 64-bit arithmetic for `addr + offset + width` and
  compares against the current length before truncating to `usize`.
- Tables check index and element type on every get/set/init/copy/fill/grow.
- Value stack, local counts, call depth and steps are capped.
- The decoder rejects oversized modules and vectors; the lexer rejects oversized
  strings and deeply nested comments; the s-expression parser caps depth.

## Resource limits

| Resource | Default | Enforced by |
| --- | ---: | --- |
| Module bytes | 64 MiB | decoder |
| Text bytes | 16 MiB | lexer |
| Instructions | 4,000,000/expression | decoder/parser |
| Nesting | 1,000 | decoder, lexer, s-expr |
| Locals | 50,000/function | decoder, interpreter |
| Value stack | 1,000,000 values | `ExecState` push |
| Call depth | 1,024 | interpreter |
| Steps | 10,000,000 | interpreter |
| Memory | 512 MiB/memory | `MemoryInst` |
| Table | 1,000,000 entries | `TableInst` |
| HTTP body | 2 MiB | server |
| HTTP headers | 64 KiB | server |

All limits are configurable in code (`Limits`, `Store`, `Server` constants);
none require a network service.

## Traps instead of panics

Runtime failures are typed `Trap` values. The interpreter uses `Result`
throughout; `unreachable!` is used only for invariants established immediately
above (for example, a validated control stack being non-empty). Fuzz tests feed
mutated modules and text to the decoder, validator, parser and instantiation
path.

## Studio token and CSP

- A 128-bit token is required for the page and every API route. It is compared
  as a string; the threat is local, not timing-sensitive.
- Static JS/CSS are token-free because they contain no state, but they are
  served only on loopback.
- The page cannot be framed (`X-Frame-Options: DENY`), MIME sniffing is
  disabled, and CSP blocks inline/eval/external script. This prevents a
  third-party page from embedding the Studio and reading state.
- There is no endpoint that reads files, spawns processes or opens sockets.

## DNS rebinding

The server does not use the `Host` header for routing and binds to `127.0.0.1`.
A rebinding attack still cannot read responses without the token, which is
delivered in the URL fragment-free query only to the initial navigation. The
token is never written to disk.

## Honest limits

Vela is not hardened against a compromised local account or browser. It is a
developer workbench. If you need isolation, run it in a container or VM and do
not expose the port through a proxy.
