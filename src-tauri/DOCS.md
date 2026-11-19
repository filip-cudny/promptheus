# Backend (Rust + Tauri 2)

## Layout

Directories follow their names (`commands/`, `services/`, `models/`, `setup/`). Files whose place does not follow from their name:

```
src-tauri/
├── Info.plist                  # macOS bundle keys merged into the .app
├── entitlements.plist          # macOS code-signing entitlements
├── resources/                  # default settings, prompts and skills, embedded into the binary with `include_str!`
└── src/
    ├── main.rs                 # shim — calls `app_lib::run()`
    ├── lib.rs                  # `pub fn run()`: plugins, setup hook, invoke handler
    └── setup/                  # app-init wiring, called from the `lib.rs` setup hook
```

## Conventions

### Entry points: `lib.rs` is the app, `main.rs` is a stub

`main.rs` only sets `windows_subsystem` and calls `app_lib::run()`. All real wiring lives in `lib.rs::run()`, which delegates to `setup/` (`setup::init::run` is the `Builder::setup` hook).

When adding plugins, state, or background tasks: extend the right `setup/` submodule, not `lib.rs` itself.

### Commands vs Services

- **Commands** (`commands/`) — thin Tauri `#[tauri::command]` handlers. Deserialize args, call a service, return `crate::Result<T>`.
- **Services** (`services/`) — business logic. Services that own windows, events or other Tauri resources take `AppHandle` and use `tauri::` types; logic that needs none stays free of them and is unit-tested in isolation.

```rust
// commands/prompts.rs
#[tauri::command]
pub async fn get_prompt(
    config: State<'_, Arc<Mutex<ConfigService>>>,
    kind: PromptKind,
) -> crate::Result<PromptDoc> {
    let config = config.lock().await;
    Ok(build_prompt_doc(&config, kind))
}
```

#### Command body discipline

A command file deserializes input, looks up state, calls a service, returns the result. Inside `commands/` do **not**:

- Implement business helpers (move to `services/`).
- Build response payloads with inline `serde_json::json!` (define a `models/` struct or extend an existing one).

Command-local `static`s are allowed for transient window state that is handed from one command to the next (e.g. the `PENDING` slot of `commands/text_preview.rs`, `commands/conversation_dialog.rs`, `commands/image_preview.rs`) or for version counters (`SETTINGS_VERSION` in `commands/settings.rs`); state with its own lifecycle belongs in a service managed in `setup::state`.

If a command file starts holding its own state machine, the logic belongs in a service.

#### Splitting a module

Split when one of these is true:

1. A file passes ~300–400 LOC and contains more than one cohesive concern.
2. Two unrelated `State<T>` flow through the same file (one module per service).
3. A subsystem has its own lifecycle (background task, watcher, plugin-like surface) — it earns its own module under `services/<name>/` with `mod.rs` exporting an `init()`/`shutdown()` (or constructor + `Drop`).
4. Tests are getting awkward — if mocking one piece requires touching unrelated code, the seam is wrong.

Don't pre-split. Three short files with one function each is worse than one 200-line file. Wait for the friction.

### Registering commands: the `handlers!` macro

Commands are registered through the `handlers!` macro defined in `commands/mod.rs`. `tauri::generate_handler!` only accepts a flat token list — concatenation must happen at macro expansion time, not via `Vec` — so we wrap the full list in `macro_rules! handlers!` and call `crate::handlers!()` from `lib.rs`. Group entries per source file with a `// === <file> ===` comment.

When adding a command: add the `pub mod` line at the top of `commands/mod.rs`, then a `$crate::commands::<file>::<fn>` entry in the matching `// === <file> ===` block of the macro.

### State Management

- One struct per concern. We **do not** use a single God-state — each service is managed individually as `Arc<Mutex<…Service>>` (or `Arc<…>` if internally synchronized) by `setup::state`.
- Commands take only the services they need: `State<'_, Arc<Mutex<ConfigService>>>`, not a monolithic handle.
- Initialize state inside `setup::init::run` (called from `Builder::setup`) when construction needs `AppHandle` (e.g. resolving `app.path()` directories). Plain values can be `manage`d before `.setup()`.

#### Mutex choice

Pick the right primitive — getting this wrong deadlocks or freezes the app:

- **Service state held across `.await` → `tokio::sync::Mutex`.** This is the default for anything reachable from an async command.
- **`std::sync::Mutex` is allowed only for short, await-free guards** — typical examples are the command-local statics that swap an `Option`, e.g. in `commands/text_preview.rs`, `commands/conversation_dialog.rs`.
- **When a `std::sync::Mutex` lives on a service struct** (e.g. `services/ai_webview` snapshot maps), every guard **must drop before any `.await`**. Holding a sync mutex across `.await` deadlocks the runtime; holding it across a streamed response stalls every other command.

Cross-reference: see `docs/gotchas/tauri-command-threading.md` for the related GTK-thread issue on Linux sync commands.

### Errors

One crate-level enum `crate::Error` in `error.rs`, one alias `crate::Result<T>`. All `#[tauri::command]`s return `crate::Result<T>` — `String` errors and `anyhow::Error` are not allowed at the IPC boundary (`anyhow` does not implement `Serialize`).

The enum is **layered**, not a single flat list:

- Each service owns its own `*Error` thiserror enum (`AiError`, `ConfigError`, `ExecutionError`, `McpError`, `SkillError`, `HistoryError`, …).
- `crate::Error` wires every service error in via `#[from]`, so commands can use `?` directly.
- Foreign error types we cross often (`std::io::Error`, `serde_json::Error`, `rusqlite::Error`, `tauri::Error`) are also `#[from]` on `crate::Error`.
- `crate::Error::Other(String)` is the explicit escape hatch for one-off cases that don't justify a new variant. `From<String>`/`From<&str>` are implemented for it.

Serialization is hand-rolled and flattens to a string (`s.serialize_str(&self.to_string())`) — the frontend treats errors as opaque user-facing messages. If a feature needs the frontend to discriminate variants, extend `Serialize` to emit `{ kind, message }` once for the whole enum, don't do it per-call site.

`anyhow` is fine **inside** services for ergonomic `?` chaining; convert at the service's public boundary into the service-level error.

### IPC channels

Pick the right channel for each direction:

| Need                                              | Use                                |
| ------------------------------------------------- | ---------------------------------- |
| Request/response, command-style                   | `#[tauri::command]` + `invoke`     |
| Backend → frontend stream tied to a single call   | `tauri::ipc::Channel<T>`           |
| Broadcast to all listeners (lifecycle, app-wide)  | `app.emit("name", payload)`        |
| Window-scoped notification                        | `app.emit_to(label, "name", …)`    |
| Frontend → backend without return value           | Still use `invoke` — events from JS to Rust are harder to type and harder to permission |

Default to commands. Reach for `app.emit*` only for true one-to-many notifications. Streamed results (AI completions, skill execution, generation events) belong on `Channel<T>` — typed, scoped to the invoke call, and back-pressure-friendly. The reference implementation is `commands/ai.rs` + `commands/execution_stream.rs`.

**Call-scoped events go on the `Channel<T>`, not on `app.emit`.** The one exception is the run lifecycle: `services/execution/skill_executor.rs` emits `execution-started` and `execution-completed` with `app.emit` in addition to the Channel, for listeners outside the invoking window. Do not broadcast other per-call events: subscribers can't tell which run they belong to.

### Async

- Tauri ships with `tokio` via `tauri::async_runtime`. Use it; don't pull a second runtime.
- `#[tauri::command] async fn` runs on a tokio task. `State<'_, T>` requires the explicit lifetime — that's not optional in async commands.
- Long-running background work (watchers, pollers): spawn with `tauri::async_runtime::spawn` from `setup::background`, not from inside a command.
- **Session-scoped tasks are the exception** — work that only lives as long as one user-triggered session is spawned from the command that starts the session (e.g. `services/speech/reminder.rs` and the level task of `services/speech/widget.rs`, spawned by `services/speech/session.rs`). Cancel them with a monotonic generation counter on the owning service (`SpeechService::session()`) rather than a cancellation channel: each tick re-checks `is_recording() && session() == captured`, so a stop, a restart, or a service reset all retire the task with no extra plumbing.
- When a command both holds a `Mutex<Service>` lock and iterates a long-lived stream, clone the cheap inner handle (typically `Arc<Provider>`), drop the guard, then iterate. Keeps the service responsive. See `commands/ai.rs` for the pattern.

### Models

- All structs crossing the IPC boundary derive `serde::Serialize` and/or `serde::Deserialize`.
- Model structs serialize in `snake_case` and their TypeScript mirrors keep `snake_case` field names; `rename_all` appears on enums (see `src/models/DOCS.md`) and on the `camelCase` payload in `services/speech/widget.rs`.

### Menu State

Context menu sections are built from one plain `MenuState` (`services/menu_coordinator.rs`) owned by `MenuCoordinator.state`:

- `MenuState` holds the inputs of the dynamic sections: `skills`, `context_items`, `is_recording`, `is_action_executing`.
- `get_context_menu_items` (`commands/menu.rs`) assigns the fields, then calls `get_menu_items`, which maps each id of `menu_section_order` to a `build_*_items` method.
- Adding a menu section = adding a `MenuState` field if it needs input, a `build_*_items` method and an arm in `get_menu_items`.

### Logging

Uses `tauri-plugin-log` (`setup/log.rs`) with the standard Rust `log` crate. Logs go to stdout, a log file in Tauri's `LogDir`, and the webview console.

- Rust: use `log::{info, warn, error, debug}` macros directly.
- Frontend: `import { error, info, warn } from "@tauri-apps/plugin-log"`.
- `attachConsole()` bridges Rust logs into browser devtools; it is called in `src/windows/{context-menu,conversation-dialog}/entry.ts` and in the `App.svelte` of `notification`, `recording-widget` and `recording-widget-overlay`.
- Default level is `Info` globally, `Debug` for the app crate. Override with `RUST_LOG` env var.
- Log at decision points and errors, not at every function boundary.

Detailed guides (load only what you need):

- [Log levels reference](../docs/logging-levels.docs.md) — when to use each level with examples
- [Data sensitivity](../docs/logging-sensitivity.docs.md) — what to log, redact, or never log per module
- [Configuration & rotation](../docs/logging-config.docs.md) — env-specific levels, targets, file rotation, performance

### Hotkey Actions

Global hotkeys are handled **entirely in the Rust backend** (`setup/shortcuts.rs` + `services/hotkeys.rs`). Do **not** route hotkey actions through `app.emit()` to the frontend — this app has no persistent main window, so frontend event listeners are unreliable.

When adding a new hotkey action:

1. Add the action string to `services/hotkeys.rs` (binding resolution).
2. Add a match arm in the action dispatcher.
3. Implement the action as a Rust async function or call an existing command directly.

### App Lifecycle

This is a **tray-only app** — no main window opens on startup. On macOS, activation policy is set to `Accessory` (no Dock icon, no Cmd+Tab entry). UI is shown on demand via:

- System tray menu (native)
- Context menu window (borderless Tauri window, hotkey-triggered)
- Prompt dialog windows (created dynamically)

The chat window is the single `conversation-dialog` host: native chat webview, strip webview `shell-toolbar` and provider tabs as child webviews (`conversation-dialog::ai-webview-{provider id}-{n}`). Tabs live in `TabList` (`services/ai_webview/tabs.rs`).

- Every entry path (context menu, tray, pill, shortcuts, "Run in dialog") brings the window up through `ai_webview::surface` (`services/ai_webview/surface.rs`): create or show the host, pick the view (keep active, native, or a new provider tab), focus.
- At most 3 provider tabs are live; making a 4th live unloads the least recently used one that is neither active nor armed with a pill message.
- A hidden provider tab is unloaded 30 min after it was last used (`services/ai_webview/unload.rs`, polled every 60 s). Unloading closes the child webview and keeps the tab with its URL and title; activating it loads it again.
- Closing the window (OS close, strip close, send-and-copy) hides it instead of destroying it: geometry is saved, every provider tab is unloaded, `DockManager` counts the host once (`host_hidden` / `host_surfaced`). Settings, history and context-editor windows are destroyed on close.

On macOS `app.show()` only unhides the app (`NSApplication unhide`) and does not activate it. A window shown from a background context (global hotkey, tray) is brought to the front with its `set_focus()` (`makeKeyAndOrderFront` plus app activation); `dialog::open_or_focus` does this for new windows on macOS.

Because the app is tray-only with multiple short-lived windows, cross-window coordination via events is legitimate (`app.emit_to(label, …)` in `provider_menu.rs`, `conversation_dialog.rs`; the app-wide `app.emit("prompt-changed", …)` in `prompts.rs`). That is **not** the events-as-substitute-for-commands anti-pattern — there is no single window from which to drive a command. Within a single window, prefer `invoke`.

### Capabilities

- `capabilities/default.json` declares permissions for the app's webviews.
- Our own `#[tauri::command]`s do **not** need a capability entry — those checks apply only to plugin APIs and the core API surface exposed to the webview.
- When adding a Tauri plugin, add its permissions here (e.g., `"dialog:default"`).
- **`fs:allow-read-text-file` and `fs:allow-write-text-file` are granted bare, with no scope entries.** The plugin allows a path only if it is in the scope built from the permission's scope entries (none here; `tauri.conf.json` has `"plugins": {}`, and the fs plugin config accepts no scope key, `tauri-plugin-fs-2.5.1/src/config.rs`) or in the runtime fs scope (`tauri-plugin-fs-2.5.1/src/commands.rs:1538-1572`, `src/lib.rs:489-490` starts it empty). The dialog plugin's `open` and `save` add each picked file, and each picked folder, to that runtime scope (`tauri-plugin-dialog-2.7.1/src/commands.rs:160-180` folders, `:192-212` files, `:250-254` save). So these two permissions reach only paths the user picked in a file dialog during the running session. Do not add other `fs:*` permissions, and do not add scope entries, without a reason: any window can then touch those paths without a picker.
- Split `default.json` per feature once it grows past ~50 lines or two windows have non-overlapping needs (e.g. extract `settings.json` for the settings dialog so the context-menu window doesn't inherit settings-only permissions).

### Configuration

- `tauri.conf.json` is the single source of truth. Use `tauri.conf.<platform>.json` for platform overrides rather than runtime branching.
- Bundle identifier `com.promptheus.desktop` (reverse-domain). Drives macOS `~/Library/Application Support/<id>/` and Linux `~/.config/<id>/`. Renaming after release is painful — don't.
- App-runtime configuration (user settings) is **not** stored via `tauri-plugin-store`. We use `services/config/` instead because the settings layer needs hot-reload on file change, schema migration on version bumps, and composition over multiple defaults files — capabilities the plugin doesn't offer. If you find yourself reaching for `tauri-plugin-store` for new settings, extend `services/config/` instead.

#### Path resolution in settings

Two contracts, one shared resolver. Picking the wrong one for a new field will silently break portability or open path-traversal holes — match the data shape, don't invent a third rule.

- **Config-relative** (in-app data files the user manages alongside `settings.json`): resolved through `services/config/path.rs::resolve_config_relative`, which expands `${VAR}` references then enforces non-empty + non-absolute + no-`..` and joins to `app_config_dir`. Current fields: `prompt_base.*`, `surfaces.title_generation.prompt`, `surfaces.speech_to_text.prompt`, `surfaces.speech_to_text.keyterms_file`. `PromptStore::resolve` wraps this and additionally requires `.md`/`.markdown`. Add new data-file fields by going through this helper, not `config_dir.join(raw)` — the latter silently accepts absolute paths because of `PathBuf::join` semantics.
- **Absolute / system-relative** (pointers to executables and dirs outside the app): MCP `command`, `args`, and `env` values. Resolved through `services/env_resolve::resolve_env_refs` only — no joining, no validation. The user owns absolute correctness. `command` additionally falls through `services/shell_env::resolve_command`, which does PATH lookup for bare names; `args` and env values do not.

Placeholders supported in any field that goes through `resolve_env_refs` (which is both contracts):

- `${VAR}` — any process env var (loaded from the user's shell + `<config_dir>/.env`).
- `${CONFIG_DIR}` — the resolved `app_config_dir`. Set by `services/config/loader::load_env` after dotenv processing, so it is reliable and not user-overridable from `.env`. Useful in MCP `args`/`env` to point at config-bundled data without hardcoding `/Users/...`. **Has no effect in config-relative fields** — `${CONFIG_DIR}/foo.md` expands to an absolute path and is rejected; write `foo.md` instead.
- `${HOME}` — provided by the OS environment; not synthesized by the app.

Cwd is **not** part of either contract. Tauri inherits the launch cwd (Finder/launchd give `/`); MCP child processes inherit ours. Never write a relative path expecting `cwd`-relative resolution — there is no consistent cwd to be relative to.

### Dependency hygiene

- `tauri` is pinned to an exact patch version — breaking changes happen at minor releases, and 2.x patches occasionally tighten internals our code touches via `unstable`/`macos-private-api` features.
- `tauri-plugin-*` crates are pinned exactly **but independently** of Tauri core. The official plugin ecosystem does not version in lockstep with core minors (no `2.10.x` line of plugins exists). The pins in `Cargo.toml` are the latest releases known to build against the core version we use; bump them deliberately, not in bulk.
- `serde`, `tokio`, `thiserror` track latest stable major lines — they're pulled in transitively anyway, divergence costs duplicate compilation.
- Before adding any dependency, verify the current latest version (`cargo search`); follow the verification process in `docs/api-verification.md`.

### Icons

- `src-tauri/icons/logo.svg` is the only hand-edited logo file. `pnpm icons` (`scripts/icons.mjs`) first writes from it `logo-mono.svg` (each stop gray `min(255, 1.075 Y + 6.8)`, Y = Rec.709 luma) and `logo-symbolic.svg` (black stops, opacity `0.35 + 0.65 ×` normalized luma; seams, rim and facet strokes dropped; source of the tray PNGs), then renders the rasters; hand edits to the two generated SVGs are overwritten. `logo-mono.svg` is used by the frontend and, at runtime, by the dock icon. After changing `logo.svg`, run `pnpm icons` and commit the outputs: `logo-mono.svg`, `logo-symbolic.svg`, `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico`, `tray-macos.png`, `tray-linux.png`, `Promptheus.icon/Assets/flame.png`. `Promptheus.icon/icon.json` is hand-written.
- The runtime dock icon (`services/dock_icon.rs`) follows `appearance.logo` (`color` → `logo.svg`, `mono` → `logo-mono.svg`, both embedded). `dock_icon::apply` runs at startup (`setup/init.rs`), in `apply_reloaded_settings` and in `finish_mutation`.
  - Linux: writes `promptheus.svg` into `$XDG_DATA_HOME/icons/hicolor/{scalable,32x32,128x128,256x256@2}/apps/` (`~/.local/share` by default), overriding the `.deb`'s PNGs behind the `.desktop` `Icon=promptheus`; GTK3 returns the SVG at every size only when all four exist. Files with equal bytes are not rewritten. When any file changed the `hicolor` directory's mtime is bumped (`set_modified(now)`): without it GNOME does not refresh the dock icon live. The override stays after uninstall.
  - macOS: `NSApplication.applicationIconImage` is set from `logo-mono.svg` (`NSImage initWithData:`; a nil image logs a warning and keeps the icon) and reset to `nil` for `color` (bundle icon). The remembered skin is re-applied by `dock::apply_dock_visible` when the Dock icon is shown.
- The macOS tray PNG is a template image: black pixels, shading only in alpha. A gray-valued opaque PNG turns into a silhouette. `tray-linux.png` is the same artwork in white.
- `Promptheus.icon` is the macOS 26 appearance-following icon (the flame layer on a solid fill from the dark `--surface-base`). The bundler compiles it with `actool` (Xcode 26). The `.icns` stays as the fallback for older macOS.
  - Node marks fd 0 close-on-exec, and the bundler starts `actool` with inherited stdin, so under the npm CLI `actool` runs with stdin closed. The `ibtoold` daemon it starts then gets a kqueue on fd 0, fails with "Bad file descriptor", and the build stops with `actool did not generate Assets.car file`. The broken `ibtoold` can stay in the background, and every later `actool` reuses it and fails too, even with a valid stdin.
  - `pnpm bundle:mac` puts `scripts/bin/` first on `PATH`; its `actool` kills any `ibtoold` and runs `/usr/bin/actool` with stdin `/dev/null`. Plain `pnpm tauri build` on macOS skips the wrapper and still fails this way; use `pnpm bundle:mac`. If `actool` crashes on the `.icon` with an `NSPlaceholderArray` exception instead, that is Tauri issue #15315: run `killall ibtoold` and retry.
- `@tauri-apps/api` must stay on the `tauri` crate's major.minor (`tauri build` fails otherwise). Upgrade only `@tauri-apps/cli` (`pnpm add -D @tauri-apps/cli@latest`), never `pnpm update`; `tauri icon` and `.icon` support need the CLI ≥ 2.11.

### Testing

- Services are plain Rust structs — unit-test them with `#[cfg(test)]` blocks like any library; that's the expected pattern.
- Commands themselves are too thin to test directly. If a command holds enough logic to warrant a test, that logic belongs in a service.
- For integration tests that need an `AppHandle`, use `tauri::test::mock_app()` — adopt only when a specific test demands it; we currently don't ship integration tests.
