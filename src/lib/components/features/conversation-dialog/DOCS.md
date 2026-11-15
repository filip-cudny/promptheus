# Conversation Dialog

Renders the chat window mounted by `windows/conversation-dialog/App.svelte`: composer, message bubbles, history sidebar, palette, tool calls.

Each entrypoint is a thin orchestrator. IPC, listeners, timers, and pagination live in drivers (`*.svelte.ts`) or services. Repeated visual fragments live in dumb primitives in `components/`. Tool-call rendering is its own subsystem under `tool-call/`.

## Layers

- **Orchestrators** (top-level files in this dir) — wire props to drivers and primitives. May call services.
- **Dumb primitives** (`components/`) — pure presentation, own `<style>`, take props + callbacks. **No** `invoke()`, `services/`, or `stores/` imports.
- **Drivers** (`drivers/*.svelte.ts`) — runes-based logic (state machines, IPC, listeners, timers). **No** JSX or CSS.
- **Tool-call subsystem** (`tool-call/`) — self-contained renderers for tool invocation UI. Only `AssistantBubble` reaches into it (via `ToolCallGroup`), apart from `components/ToolCallDetails`; the rest is internal.
- **Services** (`$lib/services/`) — IPC adapters and external-system wrappers.

## Files

Files whose role does not follow from the name:

- `AssistantBubble.svelte`, `UserBubble.svelte`, `InputArea.svelte`, `TabSidebar.svelte` — orchestrators; the two bubbles use `ActionIconButton` for the footer buttons (`active` on the Edit button, `variant="danger"` on Delete); `AssistantBubble` edits its segments through `drivers/useEditSegments`, `UserBubble` through `BubbleEditField` and `SkillEditable` with its own `editMode`/`editText`.
- `InputArea.svelte` button bar — Send is `ActionIconButton variant="primary"` (28 px filled circle, icon 16 px, stroke 3, hover; `ArrowUp`), shared with the agent composer and the `ComposerDictationBar` Finish; Stop (`variant="stop"`, `Square`) and Regenerate (`variant="muted"`, `RefreshCw`) are the muted 28 px circle; Send and copy is a plain `ActionIconButton`.
- `ContextSection.svelte` — inline context bar (text/image chips + clear/edit).
- `AttachMenu.svelte`, `components/SidebarMoreMenu.svelte` — `FloatingPanel` + `MenuList` dropdowns (the attach menu opens above its button, the sidebar menu aligns to the end of its button).
- `sidebarItems.ts` — pure helpers building `SidebarItem[]` from history.
- `$lib/utils/historySearchSnippet.ts` — `displayName`, `snippetFor`, `formatTimestamp` (Today/Yesterday/"Nd ago" scheme, not the relative one in `$lib/utils/format.ts`) for `ChatPalette`.
- `tool-call/` — self-contained tool-invocation renderer; external entrypoint `ToolCallGroup` (used by `AssistantBubble`).
- `components/ToolCall*.svelte` — pieces of `tool-call/ToolCallItem`; `ToolCallDetails` imports `tool-call/ToolResultRenderer`.
- `drivers/useConversationDialogIpc.svelte.ts` — the dialog's IPC listeners.
- `drivers/useMcpTools.svelte.ts` — finds the MCP `web_search` tool (`server.tool` id) in `listMcpTools()`, skipping tools with `disabled`; fetches at init and again on `mcp-ready` and `settings-changed`, so a tool switched off in Settings leaves an open chat window at once.
- Composer tool toggle (`InputArea` → `store.toggleTool`) updates the tab's `selected_tools` and writes only that name with `setSurfaceTool("chat", id, enabled)` (`set_surface_tool`), so a change made in Settings meanwhile is not overwritten.
- `drivers/useInputSync.svelte.ts` — local↔store sync for text/images/text attachments (see "Input sync"); `drivers/useSidebarMutex.svelte.ts` — see "Sidebar mutex".

## Layer contracts

- `components/*` and `$lib/components/shared/ui/*` **must not** import `services/`, `stores/`, or `invoke()`. Take props, emit callbacks.
- `drivers/*.svelte.ts` are pure runes — **no JSX, no CSS**. May call `invoke()`, attach window listeners, run timers, use `$effect`.
- `tool-call/*` is internal beyond `ToolCallGroup`; outside the dir, import only `tool-call/ToolCallGroup.svelte` (exception: `components/ToolCallDetails.svelte` imports `tool-call/ToolResultRenderer.svelte`).
- Orchestrators wire it all up: instantiate drivers, pass adapters as callbacks, render primitives.
- Mutex `open*` methods atomically clear other states (no `closeMenu(); editingId = …` two-step that desyncs).

## Flows

### Bubble edit (assistant)

1. User clicks Pencil → `toggleEditMode()` → `useEditSegments.enter(displayContent)` builds segmented state and resizes textareas next frame.
2. User types in any segment → `edit.onSegmentInput(idx, e)` updates that segment + auto-resizes its textarea.
3. User hits Ctrl+Enter / Save → `edit.rebuild()` collapses segments back into `{{tool_call:id}}` markers. `onContentChange(rebuilt)` propagates to store.
4. `edit.exit()` clears segments + textareaRefs.

### Scroll

Rule in `$lib/utils/scrollFollow.ts` (`reduceScroll`, tested in `tests/scrollFollow.test.mjs`), driven by `useAutoScroll` through `createScrollFollower`; mode is `follow`, `pinned` or `free`, plus a `spacer` flag.

1. `store.pinRequest` is bumped by `sendMessage` and `regenerate` only (edit-and-resend goes through `regenerate`). `useAutoScroll` then pins: the mode becomes `pinned`, `spacer` true, and the view scrolls once so the new user bubble sits at the container's top padding offset. Content changes (`trackChange`: pairs, streamed content) do not scroll while pinned or `free`.
2. `ConversationArea` wraps each `UserBubble` + `AssistantBubble` in `.pair` (`display: contents`). With `spacer` the last pair becomes a flex column with `min-height` = the container's client height (`bind:clientHeight`, exposed as `--viewport-height`) minus its vertical padding, so the reply streams into empty space and the pinned position is also the bottom. The spacer stays until the next reset or pin, so reaching the bottom does not make the content jump.
3. Following the bottom resumes only on a scroll made by the user that ends within 4 px of the bottom. A scroll counts as the user's for 250 ms after a `wheel` or `keydown` (window-level, since the container is not focusable) and while a pointer is held on the container (scrollbar drag; a scroll-to-bottom during the drag does not end it). The pin scroll clears the 250 ms window, so the pin ending at the bottom does not resume following; a scroll-to-bottom only runs in `follow` or on reset, where its landing at the bottom changes nothing.
4. Tab switch (`activeTabId`, covers load and tab close) and `switchBranch` (`store.scrollResetRequest`) reset: `follow`, no spacer, scroll to the bottom. `loadMore` keeps its scroll-height restore.

### Input sync

`useInputSync` runs four `$effect`s:

1-3. local → store on every local change (gated by `activeTabId === syncedTabId` to skip writes during the tab handover).

4. store → local on every store change. On tab change OR when text changed externally, calls `skillEditable.setTextAndHighlight` + `resetUndoStack` + (next frame) `focus + restoreCursor`. `lastDomText` tracks last DOM-driven write to detect external mutations.

This avoids feedback loops while preserving the cursor on user typing and resetting it on history-restore / tab-switch.

### Sidebar mutex

`useSidebarMutex` holds three states (`menuOpenId`, `editingId`, `confirmDelete`). Each `open*` / `start*` calls `closeAll()` first, so opening any one atomically closes the other two. Outside click (the `onclose` of `SidebarMoreMenu`'s `FloatingPanel`), the `.tab-list` scroll and Escape route through `closeMenu()` / `closeAll()` / `clear*` to keep the model consistent.

### Global context loading

`getGlobalContext()` (`stores/conversation.svelte.ts`) reads `get_context_items` and `get_labelled_context` and returns `{ text, images }`: `text` is the labelled `<pasted-text name="Text #N">` blocks (empty without text items), `images` carry the item `number`. The numbers go into the chat prompt (`[Context Image #N]`), token count, execution and history (`image_index = number - 1`). `ContextSection.saveContext` runs `unwrapContextBlocks` on the field (only `<pasted-text name="Text #N">` blocks; text outside them is kept; the `\n\n` between consecutive blocks becomes `\n`), saves it as one text item and reloads the tab from `getGlobalContext()`. Two callers:

1. `ContextSection` starts its local text/images from the active tab's stored context (so mounting never overwrites context the store already loaded) and on mount refills them from it.
2. `store.loadGlobalContextIntoActiveTab()` writes it into the active tab; `handleVoiceInput` awaits it before `sendMessage()` when `autoSend`, then shows the context section collapsed, so the auto-sent message carries the context (a window just created via `applyInitParams`, a window already open via the `voice-input` event).

### Voice-input source

`DialogInitParams` and the `voice-input` payload carry `skill_id`, `skill_model` and a nullable `invocation_source`. `handleVoiceInput` builds `/<skill_id> <text>` (plain text for an empty id), calls `store.setActiveTabInvocationSource(source ?? "alternative_execute")` and, for a non-empty `skill_model`, `store.setActiveTabModelId(skill_model)` (the active tab only, not `updateModelId`). Sources: the pill sends `pill`, the context menu's "Run in dialog" sends `alternative_execute`.

Both also carry a nullable `copy_token`, set only by pill stop + chat. With a token, `handleVoiceInput` calls `copy_chat_answer_if_unchanged(token, text)` once after the auto-sent first answer, only when the send succeeded, its tab is still open and its assistant node is not `cancelled`; `text` is the send's `result`. The backend command decides on the clipboard fingerprint. On `true`, `store.markAutoCopied(nodeId)` sets `autoCopiedNodeId` for 1200 ms: `ConversationArea` passes `autoCopied` to that `AssistantBubble`, which shows its footer and the Copy button's check. No toast. A window that reconnects (accepted limit in "One window") or is closed before the answer ends copies nothing. Follow-ups, regenerate and "Send and copy" never carry a token.

## Dictation

`InputArea` has a mic (`Mic`, left of Send and copy) that records into the composer without the pill (backend: `src-tauri/src/services/speech/DOCS.md` "Composer dictation"; unrelated to "Voice-input source"). `useComposerDictation({ owner: "chat", insert })` (`shared/widgets/`, shared with the settings agent composer) holds the state `idle | recording | transcribing | error`.

- Press: `start(store.activeTabId, skillEditable.getSelection())` saves the tab id and the textarea selection and calls `start_composer_dictation`. Until the transcript is inserted, the session is cancelled or the error dismissed, `SkillEditable` is `readonly` and Enter sends nothing (also Ctrl/Cmd+Enter).
- Bar: while the state is not `idle`, `ComposerDictationBar` replaces the button bar: level meter and `m:ss` with Cancel and Finish (the send circle); "Transcribing…" with Cancel; the error message with Retry and Dismiss (a start refusal: Dismiss only).
- Insertion: the transcript goes to the tab that started the session, whichever tab is active when it arrives: `store.insertInputText(tabId, start, end, text)` replaces the saved selection of that tab's `input_text` (`insertText`; a closed tab drops it). `useInputSync` shows it when the tab is active, and `InputArea` then focuses the field and restores the caret after the inserted text (one frame after the sync's own caret-to-end). An empty transcript inserts nothing. The transcript is never sent, copied or logged.
- Busy: the mic is disabled while `dictation-busy-changed` / `get_dictation_busy` name another owner, with the tooltip "The pill is recording" or "Dictation is running in Settings".
- Unmount of `InputArea` while recording or transcribing calls `cancel_composer_dictation`; hiding the chat window is cancelled by the backend (`composer-dictation-cancelled` resets the composer).

## One window

`conversation-dialog` is the only chat window; no command takes a host label. `open_conversation_dialog` writes `PENDING` only when it creates the window; when the window exists (visible or hidden) it goes through the surface step and delivers by event, so input is never re-sent.

Accepted limit: when the one global live-stream slot is set, a window created at that moment reconnects (`tryReconnect`) and ignores the result of `get_dialog_init_params` (it still invokes it), so an auto-send request ("Run in dialog", the pill's "Stop and chat") opens the dialog without sending.

## Tabs

Native chat tabs and provider tabs share the strip of `shell-toolbar`; the conversation store stays the owner of native tabs.

- Native sync: `App.svelte` reports a snapshot (`report_native_tabs`: `tab_id`, title, `streaming`, `listed`) after every change of the store's tabs, after `shell:active-changed` and once after mount. A tab carries `shown`, set when it is the active tab while the native view is visible and never reset; `listed` = `shown` or not clean. Clean tabs never shown (the store's first tab, the replacement after closing the last tab) stay out of the strip. `nativeViewVisible` is read from `get_tabs` after mount.
- New native tab: `addTab()` reuses only a clean tab never shown. Rust emits the chat event of a new native chat (`new-conversation`, the reuse events of `open_conversation_dialog`) before showing the native view, so the new tab is created while the view is hidden.
- Rust drives the store through events: `chat:activate-tab` (`{ tab_id }`) switches the store's tab, `chat:close-tab` (`{ tab_id, next_tab_id }`) removes it without picking a successor; `new-conversation` with `reason: "tab"` runs `addTab()`.
- `TabSidebar` lists history only; clicking an entry restores it with `store.restoreFromHistory`, deleting an entry also closes its tab.
- Closing a tab (strip, shortcut) never deletes its history entry; only the drawer's delete does.
- Shortcuts (`$lib/shortcuts.ts`, handled in the chat webview, the strip and the provider webviews): Ctrl+Tab / Ctrl+Shift+Tab next / previous (wraps), Cmd/Ctrl+1–8 the n-th tab, Cmd/Ctrl+9 the last, Cmd/Ctrl+T a new tab of the active tab's provider (native when the active tab is native; nothing when no tab exists), Cmd/Ctrl+W close the active tab.
