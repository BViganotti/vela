# Troubleshooting

## The Studio did not open a browser

The launcher prints the URL on a single line:

```
VELA_STUDIO_URL=http://127.0.0.1:PORT/?token=...
```

Auto-open uses `open` on macOS, `xdg-open` on Linux and `cmd /C start` on
Windows. If no opener is installed, copy the URL into a browser. Set
`VELA_NO_OPEN=1` to skip auto-open (used by tests). The token is required; a
page without it receives `403`.

## `403 missing or invalid token`

- Use the full URL including `?token=...`.
- If driving the API manually, send `X-Vela-Token: <token>` or `?token=`.
- Tokens are regenerated for every process start; restarting the Studio
  invalidates old tabs.

## `403 cross-origin request rejected`

POST requests with an `Origin` header must match the server origin
(`http://127.0.0.1:PORT` or `http://localhost:PORT`). This blocks cross-site
requests. If you use a proxy, do not rewrite the `Origin` header; use the
loopback URL directly.

## `405 method not allowed`

Known API routes accept only their documented methods. For example
`POST /api/health` returns 405; use `GET`.

## The module does not load

The error box shows the first failure stage:

- `parse error` — WAT is outside the supported grammar; see
  `docs/wat-grammar.md`.
- `validation error` — the module parses but violates typing, indices or limits;
  check the function/instruction offset in the message.
- `import ...` — the Studio instantiates modules with an empty registry, so
  imports are not available in the editor.
- `unknown export` — the selected export name does not exist or is not a
  function.

## Runs stop immediately at step 0

You likely have a function breakpoint on the current function. Clear
breakpoints or press Resume (which ignores the breakpoint at the current pc
once).

## Rewind is slow or unavailable

Reverse stepping restores a checkpoint and re-executes forward. Checkpoint
interval in the Studio is 32 steps, so a rewind costs at most 32 steps. Very
large memories make snapshots heavier; the Studio caps runs at 300,000 steps
and steps at 5,000 for SSE streams.

## A trap appears instead of a result

Traps are typed and intentional: out-of-bounds memory access, division by zero,
integer overflow, invalid float-to-int conversion, null/typed indirect calls,
stack overflow and fuel exhaustion. Inspect the operand stack and locals at the
failing instruction.

## Spec dashboard shows failures

The dashboard runs the fixtures compiled into the binary. A failure lists the
script line and the failed expectation. If you edited fixtures locally, rebuild
the Studio so `include_str!` picks them up. The official spec suite is not
included, so dashboard totals are for local fixtures only.

## SSE stream stops early

Streams are capped at 2,000 events and 5,000 VM steps to keep the single-threaded
server responsive. The full run remains available through Run/To end and the
trace JSON link.

## Playwright test cannot connect

- Build the release Studio first: `cargo build --release -p vela-studio`.
- The test reads `VELA_STUDIO_URL` from stdout; ensure `VELA_NO_OPEN=1` is set
  (the test does this).
- Python Playwright and a Chromium build must be installed. The test does not
  download anything.
