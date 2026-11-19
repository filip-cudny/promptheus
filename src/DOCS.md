# Frontend (Svelte 5 + TypeScript)

## Layout

- `windows/<name>/` — one folder per window: `entry.ts` + `App.svelte`.
- `lib/components/shared/ui/` — presentational primitives; `shared/widgets/` — reusable composites with state or IPC; `features/<feature>/` — feature UI.
- `lib/components/shared/drivers/` — runes drivers shared by features (`useListSelection`: highlighted index, scroll into view and Escape/Enter/arrow keys of a palette list).
- `features/<feature>/drivers/` — runes drivers (`use*.svelte.ts`) that keep window `App.svelte` files thin, e.g. `features/history/drivers/` (`useHistoryAudio`: audio-clip state and speech listeners; `useHistoryKeyboard`: search focus, paging and row navigation, bubble phase).
- `lib/shortcuts.ts` — shortcut table and `createHostShortcutHandler` (tab shortcuts + open palette, registered in bubble phase by the shell toolbar and chat windows); `lib/utils/platform.ts` — `isMac`, `isLinux`: the only place that reads `navigator`.
- `lib/stores/` — rune stores (`*.svelte.ts`); `lib/services/` — per-domain `invoke()` wrappers and event listeners; `lib/types/` — types mirroring Rust structs; `lib/utils/` (`format.ts`: `escapeHtml`, `formatDuration`, relative `formatTimestamp`), `lib/constants/`.
- `lib/stores/logo.svelte.ts` — the in-app logo follows `appearance.logo`; windows showing it call `initLogo()`. Logo SVGs are imported with `?url` into an `<img>`, never `?raw` (gradient ids collide when inlined twice).
- `styles/theme.css` — CSS variables; `styles/global.css` — global styles; `styles/fonts.css` — bundled font faces (see [styles/DOCS.md](styles/DOCS.md)).

## Conventions

### State Management

Use Svelte 5 runes, not legacy stores:

- `$state()` for reactive state
- `$state.raw()` for large objects to avoid proxy overhead
- `$derived()` for computed values
- `$effect()` for side effects (e.g., syncing with Tauri backend)

Store files use `.svelte.ts` extension to enable rune syntax outside components.

### Window entries

This is a multi-window Tauri app. Every window has its own HTML at the project root and, except the static `palette-backdrop.html`, a matching folder under `src/windows/<window-name>/`:

- `entry.ts` — imports `$lib/utils/platformAttribute` first (sets `data-platform="linux"` on Linux, also outside Tauri), then mounts the root component (`mount(App, { target: ... })`).
- `App.svelte` — root component for that window. Keep thin; pull feature UI from `lib/components/`.
- No inline `<style>` in a window's HTML: it would lose to the bundled `global.css` in production (the bundled stylesheet is linked after it). A transparent window overrides the `html, body` background in its App root (`:global(html), :global(body) { background: transparent; }`). Exception: `image-preview.html` uses inline `!important`.

Adding a new window:

1. Create `<name>.html` at project root with `<script type="module" src="/src/windows/<name>/entry.ts">`.
2. Create `src/windows/<name>/{entry.ts, App.svelte}`; `entry.ts` imports `$lib/utils/platformAttribute` first.
3. Add the HTML to `vite.config.ts → rollupOptions.input`.
4. Add the window label to `src-tauri/capabilities/default.json → windows` (child webviews go in `webviews`).
5. Build the window in Rust (`src-tauri/src/setup/windows.rs` for windows created at startup); `tauri.conf.json` has `"windows": []`.

### Components

- One component per file, organized by feature domain.
- Dumb reusable primitives go in `lib/components/shared/ui/` (no state machines, no feature concepts; no file there imports stores, services or `invoke`).
- Smart shared composites (state + logic, but reusable across features) go in `lib/components/shared/widgets/`.
- Feature-specific UI lives under `lib/components/features/<feature>/`. No cross-feature imports — pull into `shared/widgets/` if a component is needed by two features.
- Keep components thin — business logic lives in stores and services.
- `pnpm build` prints no Svelte compiler warning; keep it at zero.
- A `<!-- svelte-ignore -->` with several codes separates them with commas (`a11y_click_events_have_key_events, a11y_no_static_element_interactions`): with spaces Svelte 5 honours only the first code.
- A prop or state read at the top level of `<script>` to seed local state (`let value = $state(initial)`) is wrapped in `untrack(() => …)` when only the initial value is meant; when the live value is meant, read it inside a closure or `$derived` (Svelte warning `state_referenced_locally`).

### Services (IPC Layer)

- `lib/services/` has one file per domain (`settings.ts`, `history.ts`, `skills.ts`, …) wrapping `invoke()` with typed signatures.
- Components call services, not `invoke()`; this keeps the IPC boundary explicit. Preference, not enforced: direct calls exist in `windows/provider-menu/App.svelte`, `windows/recording-widget/App.svelte`, `windows/recording-widget-overlay/App.svelte` and `windows/conversation-dialog/App.svelte` (`get_dialog_init_params`).

### Types

- `lib/types/` mirrors Rust serializable structs; update it when adding a Rust command or struct.
- Mirrors use the default serde field names (snake_case), so the TypeScript fields are snake_case too. Exception: `types/toolResult.ts` is camelCase.
- The `$lib` alias resolves to `src/lib/` (configured in `vite.config.ts` and `tsconfig.json`).

### Services (Settings)

- `lib/services/events.ts` — the only place that listens to `settings-changed`, `history-changed` and `context-changed`: use `onSettingsChanged`, `onHistoryChanged`, `onContextChanged`.
- `invoke()` command names are snake_case (the Rust fn name); parameter names are camelCase (e.g. `{ providerId }`).
