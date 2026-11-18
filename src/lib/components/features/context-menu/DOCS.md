# Context Menu

Borderless popup window that renders menu items fetched from the Rust `MenuCoordinator` backend.

## Architecture

The context menu runs in a **separate Tauri window** (`context-menu` label) with its own HTML entry point (`context-menu.html` → `src/windows/context-menu/App.svelte`). This window is borderless, transparent, always-on-top, and hidden by default.

`ContextMenu.svelte` is split into three layers:

- **Dumb UI primitives** in `components/` — pure presentation, own `<style>`, no IPC/stores/services.
- **Logic drivers** in `drivers/` (`*.svelte.ts`) — runes-based state machines for window/keyboard/blur/panel concerns. No JSX, no CSS.
- **Data adapters** in `$lib/services` and `$lib/stores` — fetching, caching, and derived data.

`ContextMenu.svelte` is the orchestrator: instantiates drivers, wires adapters, lays out the template.

### Files

Files whose role does not follow from the name:

- `ContextMenu.svelte` — orchestrator: wires drivers/adapters, renders sections via dumb children.
- `itemExtractors.ts` — `groupBySection` and the extractors that pull context and last-interaction data out of `item.data`.
- `drivers/useFloatingPanelMutex.svelte.ts` — mutually exclusive settings / action menu / chat providers panels plus anchors.
- `drivers/useMenuPositioning.svelte.ts` — resizes and positions the window at the cursor (clamp via `clamp_menu_origin`), hover-enable gate.
- `drivers/useMenuBlurClose.svelte.ts` — window blur listener, suppressed-blur recheck timer, blur-grace passthrough.
- `$lib/stores/contextMenu.svelte.ts` — items, selection, open/close lifecycle, IPC listeners.
- `$lib/stores/modelsMenuData.svelte.ts`, `webviewProviders.svelte.ts`, `$lib/services/skillMetadata.svelte.ts` — data adapters for the panels.

### Layer contracts

- `components/*` and other UI primitives **must not** import `services/`, `stores/`, or `invoke()`. They take props and emit callbacks.
- `ContextMenu.svelte` opens the image preview through `openImagePreview` from `$lib/services/windowPreviews`.
- `drivers/*.svelte.ts` are pure runes — no JSX, no CSS. They may call `invoke()` (positioning calls `show_context_menu_panel` / `focus_context_menu`) or attach window listeners (blur).
- Mutex `open*` methods atomically close the other two panels, removing the previous bug-prone `closePanels(); settingsOpen = true;` pattern.
- `skillMetadata` cache is a module-level singleton (`$state`) so it survives menu close/reopen — fewer redundant `get_skill` IPC calls.

### Window configuration

Built in `src-tauri/src/setup/windows.rs` with label `context-menu`. Key properties: `decorations: false`, `transparent: true`, `alwaysOnTop: true`, `visible: false`.

## Interaction flow

1. Trigger (tray "Show Menu" or the global hotkey, `execute_hotkey_action` in `services/hotkeys.rs`) calls `show_context_menu_window` in Rust (not an `invoke()` command).
2. Backend positions the window at cursor, emits `show-context-menu` event to the window, then shows and focuses it.
   - **Extension variant (GNOME Wayland)**: the show payload has `shell_placement: true` (`shellPlacement`) and carries the `GetPointer` result (cursor and work area). `useMenuPositioning` clamps the position to the work area (Rust command `clamp_menu_origin`, `services/monitor.rs::clamp_origin`, shared with the recording widget; no clamp arithmetic in the frontend) and calls `place_context_menu { x, y, first }` instead of `setPosition` + `show_context_menu_panel` + `focus_context_menu`. `first: true` sends `PlaceWindow` before `win.show()` and awaits the reply; the frontend clears the first-placement flag right after starting that invoke. After the height correction it calls `place_context_menu` with `first: false`. See [linux-wayland-gnome-extension.md](../../../../../docs/gotchas/linux-wayland-gnome-extension.md).
3. Store's `init()` listener catches the event → calls `openMenu()`.
4. `openMenu()` invokes `get_context_menu_items` command and populates state (window is already visible).
5. User interacts via keyboard or mouse → executes item via `execute_menu_item` command.
6. After execution (or Escape/blur), `closeMenu()` hides the window.
   - **Focus on macOS**: showing the panel (`macos_panel::show_panel_without_activating`) activates Promptheus, so every show remembers the pid of the app frontmost before it, unless Promptheus itself is frontmost (a repositioning show while the panel is key); then the remembered pid stays. `hide_context_menu_panel { restoreFocus }` re-activates that app when `restoreFocus` is true and the panel still had focus at the hide (panel key and Promptheus active); after a click into another app or one of our windows, that one keeps focus. Items that open one of our windows right after the hide (Chat, chat provider, settings, history, open in chat from the recent list, open/run in dialog) call `closeMenu(false)`, so the previous app is not activated before our window's `set_focus`.

## Keyboard shortcuts

| Key | Action |
|-----|--------|
| Arrow Up/Down | Move selection highlight |
| Enter | Execute selected item (Shift+Enter runs a skill in the dialog on the clipboard text) |
| Escape | Close menu (closes any open floating panel first) |
| 1-9 | Quick-select item by position (runs at once when no longer number is possible (`digit × 10 > skill item count`), otherwise waits `menu_digit_select_delay_ms` (default 200, 100–1000, read from settings each time the menu opens) for a second digit; logic in `src/lib/utils/digitSelect.ts`) |
| Shift+1-9 | Same as above; runs the numbered skill in the dialog on the clipboard text |

All keyboard handling of the menu lives in `drivers/useMenuKeyboard.svelte.ts`. While the recent list of the Last interaction section is open, its filter input owns the keys:

| Key | Action in the open recent list |
|-----|--------------------------------|
| (open) | The filter input is focused when the list opens |
| Arrow Up/Down | Move the selected row (`useListSelection`) |
| Enter | Copy the selected row's main text (transcript, or the quick action's output); a failed row copies nothing |
| Escape | Clears the filter; with an empty filter closes the list (the menu stays open) |
| Digits, letters | Type into the filter; they do not run prompts while the list is open |

The input's `keydown` calls `stopPropagation()` for every key except Shift, so the menu's window handler never sees them (bubble phase, per the rule in the root `CLAUDE.md`). Clicks on a row, `In`/`Out`, `↗` and `↻` call `preventDefault()` on `mousedown`, so focus stays in the input and typing continues after a click. After the list closes the input is blurred and the menu keys work as before.

Shift on a skill (click, Enter, digit, or the action-menu item) is "Run in dialog": the text comes from `get_clipboard_text`, `openConversationDialog` opens or reuses the dialog with auto-send, source `alternative_execute`. An empty or unreadable clipboard falls back to "Open in dialog", which opens the chat through `open_conversation_dialog` (the one chat window). Shift on the skill being recorded stops the recording. "Run with transcription" in the action menu is the voice run. Shift+click on Chat is a normal click.

## Floating panels

Three mutually exclusive panels (`drivers/useFloatingPanelMutex.svelte.ts`):

| Panel | Trigger | Contents |
|-------|---------|----------|
| Settings | LMB on Settings toggle row | `SettingsPanel` — quick action model + STT model |
| Skill action menu | RMB on a skill row | `SkillActionMenu` — open in dialog / run in dialog / run with transcription / metadata |
| Chat providers | RMB on Chat row | `ProviderMenuList` — Promptheus + webview providers; picking one opens a new tab of it in the one chat window (`newTab`) |

Closing the Settings or Skill action menu triggers `menu.resumeClose()` to release the suppression any inner control (e.g. ModelSelector) may have set.

## Rich item types

The item rendering loop checks `item.item_type` and delegates to specialized components:

### ContextSection (`ContextSection.svelte`)

Renders when `item.item_type === "context"`. Extracts `ContextItem[]` from `item.data.items`.

| Element | Description |
|---------|-------------|
| Section header | `SectionHeader` (`components/`): "Context" label + item count badge (when not empty), action buttons in its `actions` snippet |
| Text chip | `ContextItemChip` (form E2): `Type` icon, `#n`, text ellipsized by CSS on one line, `title` = full content, `X` removes. Click copies the content; the `Type` icon shows `Check` for 1200 ms (`useCopyFeedback`) |
| Image chip | `ContextItemChip`: 16 px thumbnail, `#n`, `X` removes. Click opens the preview (`onOpenImagePreview`). Image chips come first, then text chips, each group in context order; `onRemoveItem` and the copy feedback use the item's index in `items` (context position), not its position on screen |
| Replace button | Replaces context with clipboard content via `setContextFromClipboard()` |
| Append button | Appends clipboard content to context via `appendContextFromClipboard()` |
| Edit button | Opens the context editor window via `openContextEditor` (`$lib/services/contextEditor`) |
| Copy button | Copies concatenated text context to clipboard (disabled when no text items) |
| Clear button | Calls `clearContext()` to remove all context items (`variant="danger"`, disabled when empty) |

Action buttons are always visible (not gated by empty state) since Replace/Append/Edit are useful even with no context.

The section updates reactively because `contextMenu.svelte.ts` refetches the menu items on the `"context-changed"` Tauri event.

### Skill list

The menu lists only skills whose `offered_in` is absent or contains `context_menu` (`SkillMenuProvider`). `get_context_menu_items` refreshes the provider's skill list from `SkillService` on every open, so created, deleted, edited, reordered and synced skills appear without a restart.

### Skill execution

When a `skill` item is clicked, the context menu store intercepts it (instead of calling the generic `execute_menu_item` backend command) and delegates to the execution store's `startExecution(skillId, "context_menu")`. The menu closes immediately, and the execution runs asynchronously.

During execution, skill items are disabled. The store applies this by overlaying `enabled: false` on skill items when `isExecuting()` is true (in `contextMenu.svelte.ts`). On `execution-completed`, the store refreshes items so they re-enable on next open.

The execution store is initialized in `src/windows/context-menu/App.svelte`.

### LastInteractionSection (`LastInteractionSection.svelte`)

Renders when `item.item_type === "last_interaction"`. The backend injects last interaction data into `item.data` in the `get_context_menu_items` command (from `HistoryService`). The section is one row, with no header:

| Element | Description |
|---------|-------------|
| `Input` chip | Preview of the latest quick action's prompt (copy on click); tooltip "Copy the last quick action's input" |
| `Output` chip | Preview of the latest quick action's response (copy on click); tooltip "Copy the last quick action's output" |
| `Transcript` chip | Latest speech entry, stateful (see below); tooltip "Copy the last transcript" |
| `▾` (`ChevronDown`) | Opens or closes the recent list; `ActionIconButton` "Earlier items", `active` while open |
| History icon | `ActionIconButton` "View execution history": opens the History window |

`item.data` carries **previews only**, capped at 200 characters. Clicking a chip calls `copyLastInteraction(chip)` → the `copy_last_interaction` command, which resolves the full text from the stored conversation tree and writes it to the clipboard. The recent list follows the same rule: its rows carry previews and copying goes through `copy_history_entry_part` (`copyHistoryEntryPart` in `$lib/services/history`), which resolves the full text of one part (`input` or `output`) of one entry by id, strips the skill prefix from the input, and refuses the output of a failed row. Never put chip or row text on the clipboard from the frontend: the stored `output_preview` is a summary, and copying it hands the user a truncated response. Keeping the full text out of `item.data` also keeps long responses off the IPC path on every menu open.

The section flashes the check icon of a copied chip or row through `useCopyFeedback` (`$lib/utils/copyFeedback.svelte`, 1200 ms).

Chips are disabled (grayed out) when there is no preview, which is also the signal that there is nothing to copy. The store refreshes on `"history-changed"` events so chips update after each execution.

The `Transcript` chip is driven by `data.transcription`, a state object rather than plain text:

| `status` | Chip |
|----------|------|
| `ok` | Copy icon, label `Transcript`, click copies the transcript |
| `retrying` | Spinner + "Retrying…", not clickable |
| `failed` | Alert icon in `--danger`, tooltip carries the error; a `RotateCcw` `ActionIconButton` ("Retry transcription") next to it invokes `retry_transcription` when `has_audio` is true |

The backend builds this payload in `get_context_menu_items` and marks `retrying` from `SpeechService::retrying_entries()`. The store refreshes on `speech-transcription-retry` in addition to the existing complete/error events, so the chip tracks a retry that was started from the history window.

A failed transcription is a real history entry (`success = false`), which is what gives the chip and the history row something to address. See [`src-tauri/src/services/speech/DOCS.md`](../../../../../src-tauri/src/services/speech/DOCS.md) for the clip lifecycle.

#### Recent list (`components/RecentInteractionList.svelte`, `RecentInteractionRow.svelte`)

`▾` opens a list of recent recordings and quick actions, newest first, under the row. Keys: see "Keyboard shortcuts".

- **Data**: `get_recent_quick_actions(100)` (`getRecentQuickActions`, `RECENT_LIMIT` in `ContextMenu.svelte`) returns the newest 100 `quick_action = 1` entries of both types with previews, `status` (`ok` / `failed` / `retrying`) and `has_audio`; see `src-tauri/src/services/DOCS.md`. The section fetches when the list opens and again on each change of its `data` prop while open (the store refreshes it on `history-changed` and `speech-transcription-*`, so a finished retry shows up); an older fetch that returns late is dropped. Closing the list clears the rows. Entries older than the newest 100 need the History window.
- **Filter**: `rankByQuery` from `$lib/utils/fuzzyRank.ts` (`fuzzyMatchIndices`) runs in the page, so it is instant. A contiguous substring ranks first (word-start bonus), else the query's letters in order with gaps, case-insensitive. It matches a row's first line and its second line (the skill name for a quick action) and returns code-point indices that `highlightFor` renders as highlights. An empty filter keeps the order; the field shows the match count while filtering. The list shows "No entries" or "No matches" when empty.
- **Row**: two lines. Line 1: the main text (the transcript, or the quick action's output) ellipsized, with the relative time on the right; line 2, faint: the skill display name, or `Transcript` for a recording. A type icon (`Mic` / `Pencil`) leads; it becomes a check for 1200 ms after a copy. Click or Enter copies the main text.
- **Hover or selection** replaces the time with controls: `In` and `Out` (copy the input / the output; quick actions only) and `↗` "Open in chat".
- **Failed and retrying rows**: the text is dimmed and Enter and `Out` copy nothing; a failed quick action still has `In` and `↗`, its first line being the input preview. A failed recording shows `↻` "Retry transcription" (`retry_transcription`) when `has_audio`, no `↗`; a `retrying` row shows a spinner.
- **`↗`** is handled in `ContextMenu.svelte` (`handleOpenRecentInChat`) after `closeMenu(false)`. A quick-action row opens the full conversation through `openConversationDialog(skillId, skillName, entryId)`, like the History window. A recording opens the chat with the full transcript (from `getHistoryEntry`) in the input field, not sent, with invocation source `chat`: a new tab when the current one has messages, else it replaces the draft (see `conversation-dialog/DOCS.md` "Voice-input source").
- **Size**: the box is 160 px high and scrolls inside, so the window size changes only when the list opens or closes. `positioning.resizeKeepingTop()` (`useMenuPositioning`) then resizes the window to the content, keeping its top edge so `▾` stays under the pointer; it clamps through `clamp_origin` only when the window would leave the work area, and on GNOME Wayland repositions with `place_context_menu { first: false }`. When the items refresh while the list is open, `ContextMenu.svelte` calls `resizeKeepingTop` instead of `triggerReposition` (which anchors the skills at the pointer); a new open of the menu always repositions at the pointer, with the list closed.
- **Layering**: the section stays a dumb child. `ContextMenu.svelte` passes the callbacks `onCopyChip`, `onOpenHistory`, `onRetryTranscription`, `onFetchRecent`, `onCopyPart` and `onOpenInChat`, and binds `listOpen`; every IPC and window call goes through them.

### Other types

Speech is rendered as `MenuItemRow` (label + icon) and runs through the `execute_menu_item` backend command. Enter on the keyboard-selected item runs the same action as a click on it (`activateSelected` in `ContextMenu.svelte`): Chat opens the chat, Settings toggles the settings panel, skill and speech go to `executeItem`; the Context, Last interaction and Models items have no action of their own, so Enter on them does nothing. `MenuItemRow` also renders the Chat row (`icon` snippet, `hoverHighlight`: highlighted on mouse hover and on arrow-key `selected`) and the Settings toggle row (`SettingsToggleRow`: chevron `icon` snippet, `trailing` snippet with the settings-window button).
