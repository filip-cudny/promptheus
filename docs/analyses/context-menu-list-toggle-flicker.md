# Context menu flicker on the recent list toggle

On GNOME Wayland the top part of the context menu vanishes for a fraction of a second when the `▾` "Earlier items" button expands or collapses the recent list. Question: what causes it on both expand and collapse, and can it be fixed deterministically.

## Diagnostic build

A production build of the feature branch logs one line per toggle, 700 ms after the click, at `info`, prefixed `[ctx-menu-diag]`, followed by JSON `{ kind, switches, entries, frames }`. Every time is `performance.timeOrigin + performance.now()` in ms since the epoch, so entries and frames share one clock. No user text is logged.

`kind` is `expand` or `collapse`. `entries` are events in order of occurrence:

| Entry | Meaning |
|-------|---------|
| `click` | The `▾` button's click handler, before `listOpen` changes |
| `after-tick` | `resizeKeepingTop` after `tick()`: `scrollHeight` of the menu, `innerHeight` of the window, `placedOriginX`/`placedOriginY`, `bodyScrollTop` |
| `clamp` | `clamp_menu_origin` resolved: returned `x`, `y` and the `height` sent |
| `delay-done` | The 500 ms wait before `setSize` ended (delay switch only) |
| `setSize-start`, `setSize-done` | Before and after `win.setSize` resolves |
| `place-start`, `place-done` | Before and after `place_context_menu` resolves (extension `move_frame` reply) |
| `setPosition-start`, `setPosition-done` | The same on the non-shell path |
| `focus`, `focus-done` | The filter input's `focus()` on mount: input `getBoundingClientRect().bottom`, `innerHeight`, `bodyScrollTop` before; `bodyScrollTop` after |
| `scrollIntoView` | The list's `scrollIntoView({ block: "nearest" })` on the selected row: `index`, row `bottom`, `innerHeight`, `bodyScrollTop` |
| `scroll` | A `scroll` event anywhere in the document (capture): `target` tag, `targetClass`, `scrollTop` |

`frames` has one sample per animation frame for 600 ms after the click: `innerHeight`, `bodyScrollTop`, `docScrollTop`, `menuTop` (`.menu-shell` `getBoundingClientRect().top`).

The Rust line `PlaceWindow(...)` (debug) is written after the D-Bus reply, so read it as reply time, not `move_frame` time.

Two runtime switches, read at startup, value `1`:

- `PROMPTHEUS_CTX_DIAG_NO_SCROLL`: `overflow: clip` on `html, body` and `preventScroll: true` on the list's `focus()`, so no programmatic document scroll is possible.
- `PROMPTHEUS_CTX_DIAG_DELAY_RESIZE`: 500 ms wait before `setSize` in `resizeKeepingTop`. A vanish at the click points to the DOM change; a vanish 0.5 s later points to the resize, paint or placement.

At startup the build logs `[ctx-menu-diag] switches noScroll=<bool> delayResize=<bool>`.

## Run instructions

- Binary: `~/.cache/promptheus/target/release/promptheus`.
- Quit the installed app first (`tauri-plugin-single-instance`, same identifier: a second process exits silently); restart it after the run.
- `node_modules` in the feature worktree: the front agent runs `pnpm install --frozen-lockfile --ignore-scripts` there before the run.
- The build uses the real config dir, `history.db` and the installed GNOME extension as is.
- Three runs, each `<binary> 2>&1 | tee /tmp/ctx-menu-diag-<config>.log`, with `<config>` and switch:
  - `plain`: no switch;
  - `noscroll`: `PROMPTHEUS_CTX_DIAG_NO_SCROLL=1`;
  - `delay`: `PROMPTHEUS_CTX_DIAG_DELAY_RESIZE=1`.
- Per run: open the menu mid-screen, then near the bottom of the work area; 3 expands and 3 collapses each. Note per case whether the top vanished and, with the delay switch, whether at the click or 0.5 s later. Note the section order (Settings → menu sections).
- Extraction: `grep -F '[ctx-menu-diag]'` and `grep -F 'PlaceWindow('` on each log.

## Findings

Pending the run.
