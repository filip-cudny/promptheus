# Screenshot generator

`pnpm screenshots [name…]` renders windows of the app from invented fixtures in headless Chrome into `docs/screenshots/<view>.png` and `docs/screenshots/<animation>.webp`; no name renders all views and animations. Needs Node ≥ 22 (built-in `WebSocket`), Google Chrome and, for animations, `ffmpeg`.

- Chrome path: `CHROME`, else `google-chrome` on Linux and `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` on macOS.
- `run.mjs` starts Vite without the project's `vite.config.ts`, with an empty `envDir` and every `VITE_*` variable removed, injects `mocks.ts` before each window's entry script and drives Chrome over the DevTools protocol with a fresh temporary profile.
- `mocks.ts` installs `mockWindows` and `mockIPC` (events mocked) and answers each `invoke` from the view's fixture in `views/<view>.ts`.
- `get_settings_catalog` is parsed from `src-tauri/src/services/config/catalog.rs` at each run (`catalog.mjs`); a line it cannot parse fails the run.

## Views

- `settings`: Models section with the settings agent panel widened after one invented exchange.
- `context-menu`: the context menu with four prompts, last interaction and context, clipped to the menu.
- `conversation`: the chat window with a two-turn conversation restored from history, the first turn an `/explain-code` skill request on a code snippet, the last answer holding a highlighted code block.
- `palette`: the command palette after `palette:show` with two providers and three tabs, clipped to the palette.
- `history`: the history window with results for the query `search`, typed into its search field.
- `setup`: the settings window with the first-run setup banner on step 4 (shortcuts), steps 1-3 done.
- `recording-pill`: the WebKit recording pill while recording, on a mid-grey backdrop.

`history` reuses `chatEntry` from `views/conversation.ts`, so a change to the conversation fixture's turns needs `pnpm screenshots conversation history`.

## Animations

- `quick-action`: text selected in an editor, `Ctrl+F1` opens the context menu, prompt 2 runs, a toast reports the result, `Ctrl+V` pastes it.
- `dictation`: an image set as context, `Shift+F1` starts the recording pill, the transcript goes to the chat, which shows the answer.

`pnpm screenshots quick-action dictation` renders only them; with no name the run also renders the PNG views, so it needs `ffmpeg` too.

- `ffmpeg` with the `libwebp_anim` encoder comes from `FFMPEG`, else `PATH` (`brew install ffmpeg` on macOS). A missing or encoder-less one fails the run before Chrome starts.
- `host.html` + `host.ts` is the page Chrome loads: it puts the backdrop of `animations/<name>.ts` on the stage, creates each window as an iframe of `/src/windows/<page>.html?view=<view>` and drives them from the host (calling the iframe's `show()`, dispatching key events, emitting events through its Tauri internals).
- `animations/<name>.ts` exports `backdrop` (HTML), `windows` (iframe page, view, position, size) and `steps`: each has a `caption` (drawn by the host page, `null` for none), `shownAtMs`, `durationMs` and timed `actions`.
- The iframe views (`quick-action-menu`, `notification`, `dictation-pill`, `dictation-chat`) are registered only in `VIEWS` of `mocks.ts`; the canvas size of each animation is in `ANIMATIONS` of `run.mjs`.
- One capture loop takes a PNG per step of the timeline at ≤ 15 fps (`MAX_FPS`), `animate.mjs` encodes them with `ffmpeg` as lossy animated WebP (quality 75) that loops forever.
- `--steps <dir>` also writes the first frame captured after `shownAtMs` of each step as `<dir>/<name>-<n>.png` (n from 1), to look at the steps.
- After each file the run prints frames, width, duration and loop count, read from the WebP chunks. The width is the canvas width × 2 (device scale factor), so ≤ 1600 px for the 800 px canvases. The targets 8–12 s and ≤ 3 MB are not checked by the run; check the printed duration and `ls -l`.

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
