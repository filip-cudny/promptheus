# Promptheus Tauri — Documentation Index

Top-level index of the documentation files in the project. Updated incrementally as new areas are documented.

## Project Structure

- [README.md](README.md) — product overview with screenshots, install, first run, configuration and settings sync, links to developer docs
- [src/DOCS.md](src/DOCS.md) — Svelte frontend: layout and conventions
- [src-tauri/DOCS.md](src-tauri/DOCS.md) — Rust backend: layout and conventions
- [src/styles/DOCS.md](src/styles/DOCS.md) — Styles: theme variables, font stacks, bundled Inter and JetBrains Mono
- [scripts/screenshots/DOCS.md](scripts/screenshots/DOCS.md) — Screenshot generator: run, fixtures, adding a view
- [src-tauri/src/models/DOCS.md](src-tauri/src/models/DOCS.md) — Data structures: derive standard, stable `settings.json`, serde defaults, IPC model structs, adding settings fields, skill `offered_in`
- [src-tauri/src/services/DOCS.md](src-tauri/src/services/DOCS.md) — Service layer: service file layout and conventions
- [src-tauri/src/services/change_log/DOCS.md](src-tauri/src/services/change_log/DOCS.md) — Change log: one immutable JSON file per settings-agent operation, entry format, pruning, sync staging
- [src-tauri/src/services/settings_agent/DOCS.md](src-tauri/src/services/settings_agent/DOCS.md) — Settings agent: tools, tool loop, session lifetime, proposal and secret events, commands, secret guarantees
- [src-tauri/src/services/mcp/DOCS.md](src-tauri/src/services/mcp/DOCS.md) — MCP client: rmcp wrapper, McpClient, McpRegistry, McpError
- [src-tauri/src/services/speech/DOCS.md](src-tauri/src/services/speech/DOCS.md) — Speech: failure model, audio clips, retry concurrency, commands, events, recording widget
- [src/lib/components/shared/ui/DOCS.md](src/lib/components/shared/ui/DOCS.md) — Presentational UI primitives and their conventions
- [src/lib/components/features/context-menu/DOCS.md](src/lib/components/features/context-menu/DOCS.md) — Context menu popup window: sections, keyboard nav, item execution
- [src/lib/components/features/settings/DOCS.md](src/lib/components/features/settings/DOCS.md) — Settings dialog: sidebar nav, auto-save sections, models editor
- [src/lib/components/features/conversation-dialog/DOCS.md](src/lib/components/features/conversation-dialog/DOCS.md) — Chat window: composer, bubbles, sidebar, palette, tool calls
- [docs/api-verification.md](docs/api-verification.md) — Process for verifying framework APIs against latest docs
- [docs/logging-levels.docs.md](docs/logging-levels.docs.md) — Log levels: when to use each with examples
- [docs/logging-sensitivity.docs.md](docs/logging-sensitivity.docs.md) — Data sensitivity tiers and per-module redaction rules
- [docs/logging-config.docs.md](docs/logging-config.docs.md) — Environment config, targets, rotation, performance
- [docs/gotchas/tauri-command-threading.md](docs/gotchas/tauri-command-threading.md) — Sync Tauri commands run on GTK main thread; use `(async)` for blocking work
- [docs/gotchas/paste-handler.md](docs/gotchas/paste-handler.md) — Shift+Cmd/Ctrl+V raw paste: Mac vs non-Mac paths, empty-`DataTransfer` paste into AI webviews
- [docs/gotchas/webview-drag-drop.md](docs/gotchas/webview-drag-drop.md) — AI webview builders must call `.disable_drag_drop_handler()` to receive file drops
- [docs/gotchas/linux-wayland-gnome-extension.md](docs/gotchas/linux-wayland-gnome-extension.md) — GNOME Wayland: shortcuts, positioning and focus through the GNOME Shell extension (D-Bus `com.promptheus.Shell`)
- [docs/gotchas/linux-webkit-idle-freeze.md](docs/gotchas/linux-webkit-idle-freeze.md) — AI webview content process hangs after idle and blocks the GTK main loop; memory pressure, responsiveness watchdog and unloading of idle provider tabs
- [docs/gotchas/linux-scale-change-repaint.md](docs/gotchas/linux-scale-change-repaint.md) — stale or white window drawing after a scale change between monitors on GNOME Wayland; debounced 1 px resize nudge on GTK `scale-factor` notify
- [docs/gotchas/linux-chat-window-corners.md](docs/gotchas/linux-chat-window-corners.md) — chat window rounded corners and 1 px border on GNOME Wayland: a GtkBox cairo clip is not honoured by WebKitGTK children, so the pages cut the corners and draw the border over a transparent host; the shadow is GTK CSS on that box in a GDK shadow-width margin
- [docs/gotchas/linux-nvidia-webkit-compositing.md](docs/gotchas/linux-nvidia-webkit-compositing.md) — SIGSEGV of the app when a page enters accelerated compositing on NVIDIA Linux (empty WebKit buffer transport mode); shared-memory renderer env pair set at startup
