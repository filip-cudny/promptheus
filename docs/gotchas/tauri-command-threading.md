# Tauri command threading & GTK main thread blocking

## Rule

Any `#[tauri::command]` that performs blocking work (clipboard, filesystem, subprocess, network, GTK calls) **must** be marked `#[tauri::command(async)]` or written as `pub async fn`. A plain sync `pub fn` command is invoked **on the GTK main thread** on Linux — blocking it freezes the entire app: UI rendering, IPC, heartbeat, every other command.

## Why

The `#[tauri::command]` macro picks the execution context from the attribute and the signature. Sources: `tauri-macros-2.5.5/src/command/wrapper.rs`, `tauri-2.10.3/src/ipc/mod.rs`.

| Form | Runs on | Source |
|---|---|---|
| `#[tauri::command] pub fn ...` | Inline in the IPC invoke handler, which runs on the main thread (GTK loop on Linux) | `wrapper.rs:359-390` (`body_blocking`: calls the function, then `kind.block(result, resolver)`) |
| `#[tauri::command(async)] pub fn ...` | tokio task on the async runtime (`async_runtime::spawn`), a worker thread | `wrapper.rs:226`, `wrapper.rs:316-350` (`body_async` wraps the call in an `async move` block), `ipc/mod.rs:371-385` (`respond_async_serialized_inner` → `crate::async_runtime::spawn`) |
| `#[tauri::command] pub async fn ...` | Same tokio task | same lines |

`(async)` on a sync `fn` does not use the blocking pool: the body runs on a regular tokio worker.

Consequence: `(async)` moves a blocking call off the GTK main thread, so the GUI stays responsive. The call still blocks the tokio worker it runs on, and `async fn` commands that call blocking code do the same. Known blocking `async` commands: `copy_last_interaction`, `copy_history_entry_part` and `copy_history_entry_content` (`commands/history.rs:264`, `:390`, `:412`; `ClipboardService::set_text` waits for the `wl-copy` child in `services/clipboard.rs` `set_text_subprocess`). They block a tokio worker, not the GUI. The clipboard-to-context commands (`set_context_from_clipboard`, `append_context_from_clipboard`) are `async fn`s that read through `read_clipboard_item`, which runs the arboard reads in `spawn_blocking`, so they block no tokio worker either.

Sync commands run on the main thread because Tauri needs that thread to touch the WebView (GTK is not thread-safe). Any blocking call in a sync command stalls the GUI.

On Linux X11 there's an additional self-deadlock path. `arboard::Clipboard::get_text()` sends a `SelectionRequest` to the clipboard owner and blocks waiting for `SelectionNotify`. If our own WebKitGTK webview owns the selection (because the user just copied something inside the app), that request must be serviced by **our** GTK event loop — which is blocked inside `arboard`. After ~4 seconds arboard times out. Repeated invocations serialize; pressing the shortcut 5–6 times produces 20+ second freezes.

## Symptom in logs

```
[INFO][arboard::platform::linux::x11] Time-out hit while reading the clipboard.
[INFO][app_lib::heartbeat] heartbeat tick #N delay=21.87s
```

The heartbeat tick measures `app_handle.run_on_main_thread(...)` callback latency. Any multi-second delay there means the GTK loop is blocked.

## Fix pattern

```rust
#[tauri::command(async)]
pub fn get_clipboard_text() -> crate::Result<String> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| Error::Other(e.to_string()))?;
    let text = clipboard
        .get_text()
        .map_err(|e| Error::Other(e.to_string()))?;
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err(Error::Other("clipboard is empty".into()));
    }
    Ok(trimmed)
}
```

The `(async)` attribute moves execution off the main thread without forcing a rewrite to `async fn`. `state.blocking_lock()` on a tokio `Mutex` panics when called inside an async context, which includes `(async)` commands (they run as tokio tasks); use `.lock().await` in `async fn`, and no code uses `blocking_lock()`.

## When to apply

Audit any new command that:

- Touches the clipboard (`arboard`, `tauri-plugin-clipboard-manager`)
- Reads/writes files synchronously
- Spawns subprocesses and waits for them (`Command::output`, `wait`)
- Makes network calls (use `pub async fn` + `reqwest` instead)
- Calls into GTK/Cocoa APIs that may block

Cheap rule of thumb: if the function body contains any I/O that isn't a fast in-memory operation, mark it `(async)`.

## Cleaner alternative when applicable

If the data is already available in the WebView (e.g. clipboard contents during a `paste` event), prefer reading it on the JS side via `e.clipboardData` instead of round-tripping through a Tauri command. WebKit's clipboard pipeline is already on the right thread and doesn't go through X11 selections — zero risk of the deadlock above. See [paste-handler.md](paste-handler.md).
