# Context menu flicker on the recent list toggle

On GNOME Wayland the top part of the context menu vanishes for a fraction of a second when the `▾` "Earlier items" button expands or collapses the recent list. Question: what causes it on both expand and collapse, and can it be fixed deterministically.

## Method

A production build of the feature branch logged one line per toggle, 700 ms after the click, at `info`, prefixed `[ctx-menu-diag]`, followed by JSON `{ kind, switches, entries, frames }`. Every time is `performance.timeOrigin + performance.now()` in ms since the epoch, so entries and frames share one clock; times below are relative to the `click` entry. No user text was logged. The diagnostic code (`drivers/ctxMenuDiag.ts` and its marks in the list, the list selection driver and `useMenuPositioning.svelte.ts`) is temporary and is removed at the end of the branch.

Entries, in order of occurrence: `click` (the `▾` handler, before `listOpen` changes), `after-tick` (`resizeKeepingTop` after `tick()`: menu `scrollHeight`, window `innerHeight`, `placedOriginX`/`placedOriginY`, `bodyScrollTop`), `clamp` (`clamp_menu_origin` result: `x`, `y`, `height` sent), `delay-done`, `setSize-start`/`setSize-done` (around `win.setSize`), `place-start`/`place-done` (around `invoke("place_context_menu")`), `focus`/`focus-done`, `scrollIntoView`, `scroll`. `frames` held one sample per animation frame for 600 ms after the click: `innerHeight`, `bodyScrollTop`, `docScrollTop`, `menuTop` (`.menu-shell` `getBoundingClientRect().top`). The Rust line `PlaceWindow(...) -> <placed> in <duration>` (debug, `services/gnome_shell/mod.rs`) is written after the D-Bus reply, so it is reply time, not the time of the extension's `move_frame`.

Three runs on GNOME Wayland, each with the binary of the diagnostic commit, one log per run (`tee`):

| Run | Switch | Toggles logged |
|-----|--------|----------------|
| `plain` | none | 26 (13 expand, 13 collapse) |
| `noscroll` | `PROMPTHEUS_CTX_DIAG_NO_SCROLL=1`: `overflow: clip` on `html, body` and `focus({ preventScroll: true })` | 16 (8, 8) |
| `delay` | `PROMPTHEUS_CTX_DIAG_DELAY_RESIZE=1`: 500 ms wait before `setSize` | 13 (7, 6) |

Per run the menu was opened mid-screen and near the bottom of the work area and toggled by hand; the user noted what was seen per case. There was no screencast: symptoms are the user's observation, timelines are the logs. Each log's first diagnostic line, `[ctx-menu-diag] switches noScroll=<bool> delayResize=<bool>`, confirmed the run's switches, and every JSON line repeats them. The `delay` log has no toggle with a changed origin, so the near-bottom case is backed by logs only in `plain` (toggle 13) and `noscroll` (toggles 11, 15).

## Findings

### Setup measured

- Section order (Settings → Context menu → Menu section order): Chat, Skills, Speech, Last interaction, Context, Models, Settings. The `▾` button sits in the fourth section, three sections lie below it.
- Heights: the closed menu has `scrollHeight` 454 and the window `innerHeight` 456 (`after-tick` of every collapse); the open list has `scrollHeight` 648 and the window 650 (`after-tick` of every expand). A toggle changes the window by 194 px.
- Work area: the logs do not carry the work area, only the origins. The window x was 2930-3533 for a 320 px width, so the menu opened on a monitor right of x = 2930. The bottom edge is 1440: the near-bottom opens placed the closed menu at y = 984 (`PlaceWindow(.., (3180, 984), activate=true, ..)` in `plain`; 984 + 456 = 1440) and the first toggle there clamped y to 790 (`clamp` y = 790, height = 650; 790 + 650 = 1440). The work area top is not measured.
- Mid-screen toggles: `clamp` returned the `placedOriginY` it received (`plain` 659 -> 659 in all 12 toggles at that origin; `noscroll` 661; `delay` 686, 609, 625). The origin did not change, so mid-screen only the size changes.
- Near the bottom: the first toggle moved the window up 194 px (`after-tick` `placedOriginY` 984, `clamp` y 790), later toggles kept y 790 (`plain` toggles 14-24). This matches the user's note that the first click repositioned the menu and later ones stayed in place.

### Document scroll is ruled out

- In every frame of all 55 toggles `bodyScrollTop`, `docScrollTop` and `menuTop` were 0, and the logs hold no `scroll` entry. `focus-done` `bodyScrollTop` was 0 each time.
- `noscroll` (no programmatic document scroll possible) looked the same as `plain`; the user: "no difference".
- The `scrollIntoView` entries after the first near-bottom toggle (`plain` toggle 13: indexes 4, 3, 2, 0 at +606, +652, +669, +685 ms; `noscroll` toggles 11 and 15, +224 to +667 ms) are the list's own scroller; the document stayed at 0, and they come after the window already had its final size.

### Expand (all positions, all runs)

Symptom (`plain`, `noscroll`): the newly shown part is cut off first and the whole menu grows afterwards; with `delay` the list is cut for 0.5 s and then the window jumps.

Timeline, `plain` toggle 1 (mid-screen): `after-tick` +4 ms (`scrollHeight` 648, `innerHeight` 456); `clamp` +10; `setSize-start` +10; `setSize-done` +146; frames: `innerHeight` 456 until +143, 650 from +143. Over all expands `setSize-done` came at +90 to +226 ms (median +99; 13 toggles) in `plain` and +88 to +144 ms (8) in `noscroll`. In `delay` toggle 1: `after-tick` +7, `delay-done` +512, `setSize-done` +515, frame `innerHeight` 650 at +518; all 7 expands had `setSize-done` at +511 to +515.

Mechanism: the DOM grows at the click (`after-tick` shows `scrollHeight` 648 inside a window of 456 within 7 ms), the native window grows when `win.setSize` resolves, 90-226 ms later. In between the 194 px that do not fit are outside the window and are cut. With `delay` the cut lasts the 500 ms the code waits, as the user saw. This explains every observed expand symptom, mid-screen and near the bottom; near the bottom the window moves up 194 px at `place-done` (+142 ms, `plain` toggle 13), after the resize, so the menu grows at the old origin and then jumps up.

The `setSize` call itself took 84-208 ms (median 91) in `plain` and 82-131 ms (87) in `noscroll`, against 3-5 ms in `delay` with the same resize. In `delay` the list had been laid out and painted for 500 ms before the call. Inference: `win.setSize` resolves only after the webview has produced work for the new DOM (list mount, focus, first frame), so the cut lasts as long as that work; the resize itself is fast. The `scrollIntoView` mark at +142 ms, 4 ms before `setSize-done` in toggle 1, fits this but does not prove it.

### Collapse (all positions, all runs)

Symptom (`plain`, `noscroll`): the top of the menu vanishes for about 100-200 ms, every time. With `delay`: the vanish comes 0.5 s after the click, not at the click, and no empty space shows below the menu during the 0.5 s.

Timeline, `plain` toggle 2 (mid-screen): `after-tick` +8 (`scrollHeight` 454, `innerHeight` 650); `clamp` +11 (y unchanged, height 456); `setSize-start` +11; `setSize-done` +14; `place-start` +14; `place-done` +24; frames: `innerHeight` 650 at +8, 456 at +18. Over all collapses `setSize-done` came at +11 to +55 ms (median +13) in `plain`, +12 to +18 in `noscroll`, and `place-done` at most +86 ms (`plain`), +31 (`noscroll`). In `delay` toggle 2: `after-tick` +10, `delay-done` +513, `setSize-done` +517, `place-done` +531, frame `innerHeight` 456 at +522; all 6 collapses had `setSize-done` at +514 to +517.

Mechanism, as far as the data reaches:

- The DOM change is not the trigger: with `delay` the menu shrinks at +10 ms (`after-tick`) and the user saw nothing, because the window is transparent (`html, body` are `background: transparent` in the context menu window, `.menu-shell` carries the opaque background) and the menu keeps its top.
- The trigger is the native window change: the vanish moved with `setSize-done`/`place-done` to +0.5 s. `setSize` and `place_context_menu` run back to back (`place-start` equals `setSize-done` in every toggle) and finish within 14 ms of each other in `delay`, so the data cannot say which of the two causes it.
- Position change is not needed: mid-screen `clamp` returns the same origin, the extension replied to `PlaceWindow` in 0.8-3.8 ms on every toggle line of `plain` and 0.9-3.2 ms in `delay` (`noscroll`: 11.1 ms once, 1.2 ms or less otherwise; the 18-19 ms lines are the second `PlaceWindow` of each menu open, 20:07:25.631, 20:15:11.525, 20:26:07.571), and the vanish still occurred; the extension's `move_frame` to an unchanged origin is therefore not shown to matter.
- Unexplained: why the top is not drawn for 100-200 ms after the native shrink. The page-visible events end earlier: in `plain` the last logged entry of a collapse is `place-done` at most +86 ms and `innerHeight` was 456 at most +67 ms after the click, yet the user saw the top missing for 100-200 ms. What is drawn in that time (a stale or unpainted buffer after the resize, the move, or the webview's repaint) is below what the page and the D-Bus reply observe; the data rules out document scroll, the DOM change, a position change and a slow extension reply, and names the native resize/placement as the trigger. The class of defect (stale or unpainted window content after a size or scale change on GNOME Wayland) is documented in `docs/gotchas/linux-scale-change-repaint.md`, but that is a different trigger and is not shown to be the cause here.

### Does one mechanism explain every vanish?

- Expand: yes, fully (order of DOM growth and native resize; the cut part is outside the window).
- Collapse: the trigger (native resize/placement, not scroll, not DOM) is shown in all 3 runs; the drawing fault itself is unexplained, and `setSize` versus `place_context_menu` is not separated.
- Not covered by the data: the first open of the menu (`triggerReposition`, out of scope) and macOS.

### macOS pill hover path (inference, no macOS run)

Everything in this section is read from the code; no macOS run backs it.

- Opening is the reverse of the menu toggle: `openPopup` in `src/windows/recording-widget/App.svelte` awaits `invoke("set_recording_widget_preview", { open: true })` and only then, in `showPreview`, sets `previewContent`. The backend (`commands/speech.rs` -> `session::set_recording_widget_preview` -> `RecordingWidget::open_overlay`) grows the frame by the constant `PREVIEW_FRAME_HEIGHT` (166) in `apply_frame`, which on macOS is one `set_frame_physical` call (`setFrame:display:` with display = true on the main thread, `services/macos_panel.rs`). The window is large before the DOM grows, so the expand-side cut of the menu cannot occur (inference).
- Closing is the same order as the menu collapse: `hidePreview` sets `previewContent = null` at once and `closePopup` enqueues `set_recording_widget_preview({ open: false })`, which restores the pill frame through `close_overlay` -> `restore_pill_frame` -> `apply_frame`. The DOM shrinks first and the native frame shrinks after the IPC round trip.
- Whether the collapse flash exists on macOS is not known. The Linux evidence ties the vanish to the native resize of a WebKitGTK window under Mutter; `setFrame:display:` on an AppKit panel with a WKWebView is a different path, so the Linux mechanism is not shown to apply (inference). The pill's popup also grows and shrinks by design when the popup opens or closes, unlike the menu toggle, where the content change is the only reason to resize.

## Recommended fix

Keep the native window size constant across the toggle: size the context menu window once, at open, for the menu with the recent list open, and let the toggle change only the DOM (the window is transparent, the menu is the opaque `.menu-shell`).

- Removes the mechanism: no `win.setSize` and no `place_context_menu` on a toggle, so neither the expand cut (DOM growth waiting for a native resize) nor the collapse flash (a native resize at the toggle) can occur, mid-screen or near the bottom; the first-toggle move up near the bottom goes away because the origin is clamped for the full height at open.
- Determinism: deterministic for the toggle itself, provided the collapse vanish is caused by the native resize or placement, which the `delay` run shows (the vanish moved with `setSize-done`/`place-done`) without separating the two; the fix removes both. It does not depend on timing.
- Cost: the window stays about 194 px taller than the visible menu while the list is closed (the difference of the two `scrollHeight` values; the code needs the open height at open, by measurement and not by a constant, because font and zoom change it). The transparent strip under the menu takes pointer input on Wayland unless an input region is set for the visible rect (Tauri's `set_ignore_cursor_events` is all or nothing); near the bottom the menu opens clamped up to 194 px higher than the cursor-based placement even when the list is never opened.
- Verify: with the fix there is no `setSize`/`place_context_menu` call on a toggle (a debug log line or the diagnostic build's `setSize-start` marks absent); then the user repeats the `plain` run: 10 toggles each, mid-screen and near the bottom, expand and collapse, and sees no cut and no flash; clicks, hover and outside-click handling are checked in the transparent strip.

Rejected candidates from the data:

- No document scroll during the resize: no effect, scroll was never the cause (`noscroll` run, all frames 0).
- Window resize before the DOM change: removes the expand cut (the order is reversed), but the collapse flash is tied to the native resize and would remain; the `delay` run shows it appears whenever the native resize happens, regardless of the DOM change.

Coverage of the macOS pill: no. The fix removes a resize on a content toggle; the pill resizes by design when a popup opens, with a constant frame height, and already grows its window before the DOM. If a flash is reported there, it needs its own macOS run before any change.
