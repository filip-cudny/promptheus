# Promptheus Tauri

Desktop application built with **Tauri 2**.

## Rules

- Always use the latest versions of Tauri, its plugins, and all dependencies. Before adding any dependency, verify the current latest version (`cargo search`, `npm info`, etc.) — do not trust versions from task files or memory, they may be outdated.
- **Prefer official `tauri-plugin-*` over rolling your own.** Before writing a service that wraps an OS API, file dialog, shell access, autostart, single-instance, logging, updater, or persistent KV storage, check whether an official plugin already covers it. Only roll your own when the plugin is genuinely insufficient (current example: `services/config/` over `tauri-plugin-store`, justified by hot-reload + migration needs — see `src-tauri/DOCS.md` "Configuration"). When you do deviate, record the rationale in the relevant `DOCS.md`.
- When using a framework API or plugin not yet documented, follow the process in [`docs/api-verification.md`](docs/api-verification.md).
- When working in any directory, check for a `DOCS.md` file first — it contains conventions and patterns for that area, or references to detailed files for complex topics.
- Adding or changing a button: follow `src/lib/components/shared/ui/DOCS.md` → Buttons.
- **Recording pill on both platforms.** The voice recording pill has two renderers: `src/windows/recording-widget/App.svelte` (macOS, and the fallback for a non-GNOME session) and `linux/gnome-shell-extension/promptheus@promptheus.desktop/extension.js` (GNOME Wayland). Every change to its look or behaviour goes to both in the same change, so they stay near-identical; only platform-forced differences are allowed, listed in `src-tauri/src/services/speech/DOCS.md` ("macOS layout and dropdown").
- **Global keyboard shortcuts** (`window.addEventListener("keydown", …)`) must use **bubble phase** — never `capture: true`. Components that own a key in some state (open dropdown, modal, autocomplete) call `e.stopPropagation()` in their own handler. This lets the focused element win naturally; capture-phase global listeners break in-context popovers (e.g. Cmd+K hijacked from a skill autocomplete).

## Logging

Add log statements **during implementation**, not as a separate step. When writing or modifying any service, command, or store, include appropriate logging inline following the guides below. Do not add temporary debug logs — use proper levels and increase verbosity at runtime with `RUST_LOG`:

```bash
RUST_LOG=app_lib::services::ai=trace pnpm tauri dev   # trace for one module
RUST_LOG=app_lib=trace,hyper=warn pnpm tauri dev       # app at trace, silence noisy crates
```

Before adding logs, load only the guide you need from `docs/`:

- **Choosing a level?** → read [`docs/logging-levels.docs.md`](docs/logging-levels.docs.md)
- **Handling user data, API keys, or clipboard?** → read [`docs/logging-sensitivity.docs.md`](docs/logging-sensitivity.docs.md)
- **Changing log config, targets, or rotation?** → read [`docs/logging-config.docs.md`](docs/logging-config.docs.md)

Do not load all three — load only what applies to the current task.

## Code style

- **No inline comments.** Code must be self-explanatory — use clear names, small functions, and logical structure instead of comments. If code needs a comment to be understood, refactor it.
- **Exception: deliberate simplifications.** A simplification with a known limit gets a comment at the code site stating its limit, the upgrade path and the checkable condition under which it stops being correct (e.g. "more than one worker process").
- Top-level doc comments (`///` in Rust, `/** */` in TS) are acceptable only when they add real value (e.g., documenting a public API contract that isn't obvious from the signature). Do not add trivial doc comments that restate the function name.
- **Icons**: Always use `lucide-svelte` for icons. Never paste raw SVG — import the named component instead (e.g., `import { MessageSquareShare } from "lucide-svelte"`).

## Documentation convention

Documentation lives close to the code it describes:

- Each directory may have a `DOCS.md` — the entry point for that directory's conventions.
- `DOCS.md` is kept short. For complex topics it references separate files in the same directory (e.g., `auth.docs.md`).
- A top-level index at [`DOCS.md`](DOCS.md) links to all directory-level docs for discoverability.

## Development Commands

All commands run from `promptheus-tauri/` unless noted otherwise.

### Frontend (Svelte/TypeScript)

| Command | What it does |
|---------|-------------|
| `npx svelte-check` | Type-check all Svelte and TS files (0 errors = pass) |
| `pnpm build` | Vite production build to `dist/` |
| `pnpm dev` | Vite dev server |
| `node --test "tests/*.test.mjs"` | Unit tests of pure TS modules (Node ≥ 23 type stripping, no runner dependency) |
| `pnpm screenshots` | Renders the README screenshots into `docs/screenshots/` (headless Chrome, see `scripts/screenshots/DOCS.md`) |

### Backend (Rust)

Run from `promptheus-tauri/src-tauri/`:

| Command | What it does |
|---------|-------------|
| `cargo check` | Type-check Rust code (fast, no codegen) |
| `cargo test --lib` | Run unit tests |
| `cargo build` | Full debug build |

### Full app

| Command | What it does |
|---------|-------------|
| `pnpm tauri dev` | Run the full Tauri app in dev mode (frontend + backend) |
| `pnpm tauri build` | Production bundle |
| `pnpm bundle:linux` | Production `.deb` only (installs the GNOME Shell extension to `/usr/share/gnome-shell/extensions/`); output in `release/bundle/deb/` of the cargo `target-dir` set in an untracked root `.cargo/config.toml`, else `src-tauri/target/release/bundle/deb/` |
| `pnpm bundle:mac` | Production `.app` only |

### Workflow rules

- `pnpm check` does **not** exist — use `npx svelte-check`.
- pnpm scripts never install dependencies (`verifyDepsBeforeRun: false` in `pnpm-workspace.yaml`): after a `package.json` or `pnpm-lock.yaml` change, run `pnpm install` yourself.
- Do not run `cargo fmt`: the crate is not rustfmt-clean, so it rewrites ~90 unrelated files. Format changed lines by hand.
- `cargo` commands must run from `src-tauri/`, not the project root.
- `pnpm` commands run from the project root (`promptheus-tauri/`).
- Every HTML window must be listed in `vite.config.ts` → `rollupOptions.input` and added to `src-tauri/capabilities/default.json` → `"windows"` (child webviews such as `conversation-dialog` and `shell-toolbar` go in `"webviews"`). Missing the first → empty white window in production. Missing the second → all `invoke()` calls fail silently.
- `invoke()` naming: command names are **snake_case** (matches Rust fn name), parameter names are **camelCase** (e.g. `invoke("execute_menu_item", { itemId, shiftPressed })`). Recurring source of silent failures.

### Platform & architecture gotchas

These are recurring traps — each lives in its own file with full background, symptom, and fix pattern. Load the file when working in the listed area; otherwise leave it.

- [`docs/gotchas/tauri-command-threading.md`](docs/gotchas/tauri-command-threading.md) — sync `#[tauri::command]` runs on the GTK main thread on Linux; blocking calls freeze the whole app. **Load when:** writing or modifying any `#[tauri::command]` that touches clipboard, filesystem, subprocess, network, or GTK APIs.
- [`docs/gotchas/paste-handler.md`](docs/gotchas/paste-handler.md) — `Shift+Cmd/Ctrl+V` raw-text paste needs a Mac vs non-Mac split; has regressed multiple times. Also covers the WebKitGTK empty-`DataTransfer` paste workaround for AI webviews (legacy path drops image and arboard-written text → image re-read via async Clipboard API, text via `request_paste_text` host bridge, both re-dispatched). **Load when:** touching `InputArea.svelte → handleKeydown`, `src/lib/utils/paste.ts`, any clipboard Tauri command, `services/ai_webview/scripts.rs` paste injection or the `request_paste_text` host bridge in `palette.rs`, or investigating paste freezes / paste-does-nothing / "image or transcription text won't paste in Claude/ChatGPT webview" reports.
- [`docs/gotchas/webview-drag-drop.md`](docs/gotchas/webview-drag-drop.md) — Tauri's OS-level drag-drop handler intercepts file drops before the hosted page sees them; AI-webview builders must call `.disable_drag_drop_handler()` to get HTML5 drag-and-drop. **Load when:** adding/modifying a `WebviewWindowBuilder`/`WebviewBuilder` that loads external provider content, or investigating "can't drop files into Claude/ChatGPT webview".
- [`docs/gotchas/linux-wayland-gnome-extension.md`](docs/gotchas/linux-wayland-gnome-extension.md) — native Wayland clients on GNOME get no global shortcuts, window positioning or activation from GTK/Tauri; a GNOME Shell extension (`com.promptheus.Shell` on the session bus) grabs shortcuts, reads the pointer and focused app, and places the context menu. **Load when:** touching global shortcuts, window positioning or focus on Linux, or the GNOME Shell extension (`linux/gnome-shell-extension/`, `services/gnome_shell/`).
- [`docs/gotchas/linux-webkit-idle-freeze.md`](docs/gotchas/linux-webkit-idle-freeze.md) — a hung WebKit content process of an idle AI webview blocks the shared GTK main loop and freezes the whole app; three layers guard it (WebKit memory pressure, responsiveness watchdog, unloading of provider tabs idle for 30 min, live limit 3). **Load when:** touching the `services/ai_webview/` lifecycle, tab unloading, `services/webkit_pressure.rs`, or investigating an AI webview window freezing after idle on Linux.
- [`docs/gotchas/linux-scale-change-repaint.md`](docs/gotchas/linux-scale-change-repaint.md) — after a move between monitors of different scale on GNOME Wayland windows keep stale, white or smeared drawing; a plugin repaints each window with a debounced 1 px resize nudge after its GTK `scale-factor` change. **Load when:** creating or changing window creation code on Linux, `services/scale_repaint.rs`, or investigating stale, white or smeared window content after a move between monitors or a scale change.
- [`docs/gotchas/linux-chat-window-corners.md`](docs/gotchas/linux-chat-window-corners.md) — a cairo clip on the `GtkBox` holding the child webviews is not honoured by WebKitGTK children, so the chat window's rounded corners and 1 px border on GNOME Wayland are cut and drawn by the pages (toolbar CSS, native chat `clip-path`, injected `provider_corners_script`) over a transparent host; the shadow is GTK CSS on that box, in a surface margin set as the GDK shadow width, with an input region and resize band around the visible window. **Load when:** touching `services/chat_corners.rs`, chat window creation, size or resize on Linux (`services/dialog.rs`), the palette or backdrop pages, or investigating square, white or unbordered chat window corners, a missing or cut shadow, or clicks on the shadow not reaching the window below.
- [`docs/gotchas/linux-nvidia-webkit-compositing.md`](docs/gotchas/linux-nvidia-webkit-compositing.md) — on NVIDIA Linux Ubuntu's webkit2gtk leaves the renderer buffer transport empty, web views get no backing store and the first page entering accelerated compositing (ChatGPT login) kills the app with SIGSEGV; `services/webkit_renderer.rs` forces shared-memory transport at startup. **Load when:** touching `services/webkit_renderer.rs`, WebKit environment variables at startup, or investigating a crash or freeze of a web view on NVIDIA Linux.

## References

_(to be added as the project grows)_
