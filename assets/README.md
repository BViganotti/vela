# Embedded Studio assets

These files are original and compiled into the `vela-studio` binary with
`include_str!`:

| File | Purpose |
| --- | --- |
| `index.html` | workbench layout (editor, disassembly, inspectors, timeline, panels) |
| `style.css` | dark theme, grid layout, responsive rules |
| `app.js` | state fetching, rendering, actions, breakpoints, tabs, SSE, keyboard |

Constraints:

- No external fonts, images, CSS frameworks or JavaScript libraries.
- The page runs under `default-src 'self'`; there is no inline script or `eval`.
  JavaScript is a single external file served by the same loopback server.
- The auth token is read from the URL query and sent as `X-Vela-Token` for API
  calls; static assets themselves contain no state.
- DOM updates use `textContent`/escaped interpolation for values and names.

## Layout

The layout is a three-column workspace (source / disassembly+inspectors /
breakpoints+spec+diff) with a full-width timeline and trace table. At 1200px
and 820px breakpoints the right column wraps and then stacks.

## Editing

Changes are part of the binary; rebuild `vela-studio`. There is no build step
for the assets. `docs/studio.md` documents the API the UI consumes.
