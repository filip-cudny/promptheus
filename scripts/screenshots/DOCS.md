# Screenshot generator

`pnpm screenshots [view…]` renders windows of the app from invented fixtures in headless Chrome into `docs/screenshots/<view>.png`; no view name renders all. Needs Node ≥ 22 (built-in `WebSocket`) and Google Chrome.

- Chrome path: `CHROME`, else `google-chrome` on Linux and `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` on macOS.
- `run.mjs` starts Vite without the project's `vite.config.ts`, with an empty `envDir` and every `VITE_*` variable removed, injects `mocks.ts` before each window's entry script and drives Chrome over the DevTools protocol with a fresh temporary profile.
- `mocks.ts` installs `mockWindows` and `mockIPC` (events mocked) and answers each `invoke` from the view's fixture in `views/<view>.ts`.
- `get_settings_catalog` is parsed from `src-tauri/src/services/config/catalog.rs` at each run (`catalog.mjs`); a line it cannot parse fails the run.

## Views

- `settings`: Models section with the settings agent panel widened after one invented exchange.
- `context-menu`: the context menu with four prompts, last interaction and context, clipped to the menu.
- `conversation`: the chat window with a two-turn conversation restored from history, the last answer holding a highlighted code block.
- `palette`: the command palette after `palette:show` with two providers and three tabs, clipped to the palette.
- `history`: the history window with results for the query `search`, typed into its search field.
- `setup`: the settings window with the first-run setup banner on step 4 (shortcuts), steps 1-3 done.
- `recording-pill`: the WebKit recording pill while recording, on a mid-grey backdrop.

## Failures

The run exits non-zero, naming the view and the cause, on:

- a command absent from the fixture (`fixture lacks command "<name>"`; `plugin:log|*` is exempt): a stale fixture after a window started calling a new command, fixed by adding the command and an invented answer to `views/<view>.ts`;
- a page exception or a console error;
- a request to anything but `127.0.0.1` over http/ws, `data:` or `blob:`; Chrome also resolves no other host;
- an `import.meta.env` key besides Vite's built-ins.

## Adding a view

1. `views/<view>.ts` exports `label` (the Tauri window label), `fixture` (command → value, or function of the invoke args), optionally `globals` (window properties the backend injects) and `show()` (emits the show or state events of an event-driven window through `@tauri-apps/api/event`, or waits for a transient state such as a highlight to end).
2. Register it in `VIEWS` of `mocks.ts` and of `run.mjs`: HTML page, window size in CSS pixels, optionally `clip` (selector of a transparent window's visible part), `type` (text typed into the focused field after `show()`), `backdrop` (page background behind a transparent window) with `margin` (CSS pixels of backdrop around the window).
3. Run `pnpm screenshots <view>` until it exits 0, and look at the PNG.

Fixtures hold invented data only: no real names, keys, paths, prompts, skills or model ids, nothing copied from a config directory.
