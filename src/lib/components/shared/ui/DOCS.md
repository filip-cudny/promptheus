# UI Components

Reusable UI primitives shared across features. See [src/DOCS.md](../../../../DOCS.md) for general frontend conventions.

## Buttons

Rules for every button in the app.

1. **Form by place.** The form of a button depends on where it stands and on its action (rules 2–4).
2. **Icon-only** in these places, when the action has a vocabulary icon (rule 6); an action without one is icon + text. Icon-only appears nowhere else:
   - window chrome and tab strip;
   - message bubble footers;
   - toolbars in section, card and panel headers;
   - composer button bars;
   - editor toolbars;
   - per-row and per-chip actions in lists (remove, reorder, copy, retry, delete, export);
   - suffixes inside an input (clear, reveal, insert placeholder);
   - pagination.

   Reason: these controls are dense and repeated next to content that explains them; a label per button would cost the width of the content. Nielsen Norman Group ("Icon Usability") finds that only a handful of icons (close, search, home, print, share) are understood without a label, so the vocabulary of rule 6 stays short and fixed.
3. **Text-only** in:
   - dialog and inline-confirm footers;
   - actions inside banners and toasts (Retry, Dismiss);
   - segmented toggles;
   - link-style navigation, ending in `›`.

   Reason: the verb is the decision; the banner's own icon already carries its state. Apple HIG and GNOME HIG both make dialog buttons text labels naming the action, not "Yes" or "OK".
4. **Icon + text** everywhere else: a standalone action outside the places of rule 2 ("Add server", "Import .md", "Delete model" in a danger zone, empty-state actions). The icon stands left of the label.
5. **Vocabulary is closed.** Rule 6 lists every icon allowed on an icon-only button. A new icon is added to it only when its meaning collides with no listed action and its action has no listed icon. Reason: a toolbar and header-bar icon is acceptable only while it is standard (Apple HIG, GNOME HIG); a second icon for one action, or one icon for two actions, breaks that.
6. **One action, one icon** app-wide:

   | Action | Icon |
   |---|---|
   | Close window, panel, modal, tab, palette | `X` |
   | Minimize / maximize, restore window | `Minus` / `Square` |
   | Hide side panel | `PanelRightClose` |
   | Toggle conversation list | `PanelLeft` |
   | Remove item from a list or chip, clear a field, cancel a running operation | `X` |
   | Delete a stored entity or its content | `Trash2` |
   | Copy | `Copy`, confirm `Check` |
   | Send and copy | `CopyCheck` |
   | Edit / view toggle | `Pencil` |
   | Save | `Save` |
   | Add item, attachment, tab | `Plus` |
   | New conversation | `SquarePen` |
   | Regenerate an answer | `RefreshCw` |
   | Retry a failed operation | `RotateCcw` |
   | More actions | `EllipsisVertical` |
   | Previous / next | `ChevronLeft` / `ChevronRight` |
   | Send, finish recording | `ArrowUp` |
   | Stop | `Square` |
   | Pause / resume | `Pause` / `Play` |
   | Dictate | `Mic` |
   | Import / export | `Upload` / `Download` |
   | Insert placeholder or variable | `Braces` |
   | Paste clipboard into context | `ClipboardPaste` |
   | Screenshot / select text into context | `ScanSquareIcon` / `TextSelectIcon` (`$lib/icons/`, `lucide-static` wrappers taking `size` and `strokeWidth` like a `lucide-svelte` icon; `lucide-svelte` has no `scan-square`) |
   | Stop and send to chat | provider icon / `MessageSquareShare` |
   | Open in its window | `SquareArrowOutUpRight` |
   | Open history | `History` |
   | Open settings | `SlidersHorizontal` |
   | Web search toggle | `Globe` |
   | Show / hide secret | `Eye` / `EyeOff` |
   | Chip view / raw file | `Type` / `FileCode2` |
   | Diagram source toggle | `Code` |

7. **Labels** are English, sentence case, start with a verb, and name the object when the place does not ("Delete server", not "Yes").
8. **Accessible name:**
   - icon-only: `aria-label` is the label; `title` is the label too, except that a keyboard shortcut is added only to `title` ("Send (Enter)") and a disabled button may show its reason in `title` instead;
   - text and icon + text: no `aria-label`, unless it contains the visible text;
   - recording pill: every button has an accessible name (`aria-label` in the webview, `accessible_name` in the GNOME extension, same text); only row 2 actions show a tooltip, through the pill's own tooltip overlay, not `title`.

   Reason: WCAG 2.2 SC 4.1.2 "Name, Role, Value" requires every control to have an accessible name, which an icon-only button gets only from `aria-label`; SC 2.5.3 "Label in Name" requires the accessible name to contain the visible text.
9. **Destructive:**
   - icon-only gets `variant="danger"` (red hover);
   - a confirm button is text-only with `variant="danger"`.
10. **Components:**
    - icon-only through `ActionIconButton`;
    - text and icon + text through `Button`;
    - icon sizes from `ICON_SIZE` (`sm` 12, `md` 14, `lg` 16).

    No override of a shared button's look from outside the component (for example `:global()` on its class). A raw `<button>` is allowed only for controls that are not buttons in this sense: `Switch`, `Chip` and chip remove buttons, `MenuItem`, list, menu and palette rows, tabs (with their close button), disclosures, segmented toggles, dropdown triggers, link-style buttons (inline text links and `›` navigation), scrims, `WindowControls` (OS-style minimize, maximize, close with its own close hover), the recording pill, `markdown.ts` HTML strings, the settings window's floating Agent pill (`settings-dialog/App.svelte`, kept with its own look). They still follow rules 6–9, except that the Agent pill's label "Agent" is a named exception to rule 7.
11. **Icon-only sizes and the primary circle:**
    - default box 22 × 22 px (padding `--space-2`, icon `ICON_SIZE.md` 14 px): bubbles, sections, rows, inputs, editors, composers;
    - large box 28 × 28 px (icon 16 px): window and panel header controls (`AgentPanel` header, `ChatTopButtons`, `TabSidebarHeader`, `Modal` close) and the tab strip "New tab";
    - hover `--surface-overlay` in both sizes;
    - the primary action of an icon-only bar is a filled circle, at most one per bar: 28 px, icon 16 px, stroke 3, `--text-primary` background, `--surface-base` icon, with a hover; in the composers only Send and Finish (Reason: only Send should be the CTA). The recording pill keeps its own colours with the same dimensions;
    - Stop and Regenerate in the composers take the place of Send: the same 28 px circle in `--surface-overlay-strong` with the icon in `--text-secondary` (`--text-primary` on hover). Stop (while a run is in progress) is `variant="stop"`, a 12 px filled `Square`; Regenerate is `variant="muted"`, `RefreshCw` 14 px with the default stroke, not filled (a fill on an arc icon renders blobs). Reason: only Send should be the CTA; a filled white circle draws the eye to buttons that should stay available but unobtrusive.

### Button components

- `ActionIconButton` props:
  - `label` (required): `aria-label`, and `title` unless `title` is given;
  - `title` (optional): only for a shortcut ("Send (Enter)") or a disabled reason;
  - `size: "md" | "lg"` (box 22 / 28 px, icon 14 / 16 px); there is no numeric icon-px `size`;
  - `variant: "default" | "danger" | "primary" | "muted" | "stop"`;
  - `type="button"`, `{...rest}` passed to the element (`aria-pressed`, `onmousedown`), a bindable `element` for anchors and refocus;
  - `stopPropagation` on click stays.
- `npx svelte-check` fails on an `ActionIconButton` without `label`.
- `Button` takes an optional `icon`: icon left, `ICON_SIZE.md`.
- `ICON_SIZE` in `$lib/constants/ui.ts`: `sm` 12, `md` 14, `lg` 16.

## Conventions

### Component patterns

- Files here are presentational: they import no stores, services or `invoke()`. Besides types, constants, `$lib/utils` helpers and sibling `ui/` components they use only rendering libraries (`morphdom` and `mermaid` in `MarkdownRenderer`). Components that need state or IPC go in `shared/widgets/`; the exception is `ThinkingBlock`, which calls the Tauri plugin `opener` directly (open a link); saving a Mermaid SVG comes in through its `onsavesvg` prop.
- Scoped `<style>` blocks for all CSS — no global style leakage.
- Custom Svelte transitions defined inline when built-in transitions (`fade`, `fly`) don't match the required animation curve (e.g., capped opacity).

### Dynamic components

Render dynamic components using `$derived` + direct tag syntax (Svelte 5 runes mode):

```svelte
<script>
  let ActiveIcon = $derived(condition ? iconA : iconB);
</script>
<ActiveIcon size={16} />
```

For typing dynamic Lucide icon props, use `ComponentType<SvelteComponent<IconProps>>` (aliased as `LucideIcon` in `ActionIconButton`). This is the proper Svelte 5 way to type class-based components passed as props. Do not use `any` or `Component<>` — use `ComponentType<>` which handles both legacy and modern components.

### ActionIconButton

- Props, sizes and variants: see "Buttons" above. `label` is required (`aria-label` and default `title`), `title` is optional, `size` is `"md"` (default) or `"lg"`, `variant` is `default`, `danger` or `primary`, `element` is bindable, other attributes go to the `<button>`.
- `confirmIcon` swaps the icon for `COPY_FEEDBACK_MS` (1.2 s) after click through `useCopyFeedback`; a repeated click restarts the window. `confirmed` is a read-only prop that also shows `confirmIcon` (used by `AssistantBubble` auto-copy).
- Copy feedback comes from `useCopyFeedback` (`$lib/utils/copyFeedback.svelte`) or `ActionIconButton confirmIcon`, never an own timer; the check keeps the button's normal text colour, no copied-state colour.
- Borderless transparent button — no border, hover adds subtle background.
- Use `confirmIcon={Check}` for actions where the user needs visual feedback (copy, save). Omit for actions with immediate visible effect (navigate, regenerate).
- Icon sizes are defined in `$lib/constants/ui.ts` — never hardcode pixel values.
- Never pass a lucide icon `fill={cond ? "currentColor" : undefined}`: `Icon.svelte` spreads its rest props after its default attributes, so an `undefined` `fill` replaces lucide's `fill="none"`, the attribute is dropped and SVG paints every path black (open paths and arcs such as `RefreshCw`, `Mic`, `ArrowUp` turn into black shapes). Pass `"none"` for the unfilled case.
- `active` marks a toggled-on button (tint of `--info`); `variant="danger"` turns the hover red; `variant="primary"` is the filled circle; `variant="muted"` and `variant="stop"` the muted circle.

### FilePathReveal

- `path`; renders a small `…` summary that reveals the file path in a popover anchored to the right edge of its container (the title row of an editor).

### HighlightedTextarea

- `bind:value`, `highlightPattern` (placeholder regex), `unknownTokenChecker`, `placeholder`, `disabled`, `minHeight`.
- Exports `insert(token)`: inserts at the caret, keeps the caret after the token and updates the bound `value`; callers use it through `bind:this`.

### Switch

- `checked`, `label` (`aria-label` and `title`), `onchange(next)` (called with the opposite of `checked`; the owner stores it, the component holds no state), `disabled`. A 26 × 15 px `<button type="button" role="switch">`: `--accent` when on, `--text-faint` when off.

### Modal

- `title`, `width` (px, default 480), `onClose`, `children`. Backdrop, header with close button (an `ActionIconButton size="lg"`), panel; a mousedown on the backdrop or `Escape` calls `onClose`.
- The panel is a flex column; children (fields, footer) are laid out inside it. Focus is the caller's job (`onMount`).

### MenuItem

- `label`, `icon` and `shortcut` snippets, `active`, `activeStyle`, `disabled`, `onclick` (receives the `MouseEvent`). Rows inside `MenuList`.

### FloatingPanel

- Every dropdown uses it with `MenuList` + `MenuItem` rows (`ModelSelector`, `AttachMenu`, `SidebarMoreMenu`, `SelectDropdown`, the context menu panels); no component positions its own dropdown or handles its outside click.
- `visible`, `anchorEl`, `onclose` (called by a pointer-down outside the panel and the anchor), `position` (`"auto"` below when it fits; `"below"` forced; `"above"` preferred, falls back below when there is more room there), `align` (`"start"`/`"end"`: left or right edge of the anchor, only with `fitContent`), `fitContent` (content width, anchored to the anchor), `flush` (no padding, defaults to `fitContent`).
- Positioned once on open, `position: fixed`, z-index `--z-overlay`: it does not follow its anchor, so an owner that scrolls (sidebar list) closes it on scroll. A panel taller than the room on its side gets a `max-height` and scrolls inside.
- A `transform` or `backdrop-filter` on an ancestor makes that ancestor the containing block of the fixed panel.

### CollapsibleSection

- Uses `bind:collapsed` for two-way binding — parent controls open/closed state.
- `titleTooltip` is the native `title` of the chevron and the title together (one wrapping span), not of the whole header, so chips and buttons in it keep their own `title`.
- Header uses Lucide `ChevronRight`/`ChevronDown` icons for collapse indicator. The `actions` snippet renders on the right side and stops click propagation.
- Content area uses `svelte/transition` `slide` for smooth expand/collapse.
- The optional `summary` snippet renders after the title and takes the free header width (`flex: 1`, clipped); the title keeps its own width, ellipsized when short of room, and `actions` stay at the right edge with or without a `summary`. A chip in the snippet stops its click (`ContextItemChip` does), so only the other header clicks toggle.

### ContextItemChip

- One context item in form E2: kind marker first, then `#n`, text chips then the text. Image: 16 px square thumbnail (`object-fit: cover`, radius 5 px) from `src` (a data URI) then `#n`. Text: lucide `Type` icon (12 px) then `#n` then the text on one line, ellipsized at the end; without `number` the icon then the text only.
- Props: `kind` (`"image" | "text"`), `number`, `text`, `src`, `title`, `icon` (replaces `Type`, e.g. `Check` after a copy), `maxWidth` (text chips only), `onclick` (renders a `<button>`, stops propagation; without it a `<span>`), `onremove` with `removeLabel` (the `X` button is rendered only when `onremove` is given; it stops propagation).
- Look K1: 20 px high, no visible border (1 px transparent, so the box keeps its size), `--accent-bg-soft` fill, hover fill `--accent-bg`, number and kind icon (`Type` or the replacing `icon`) in `--accent`, text `--text-primary`, `--radius-chip` (7 px, the same radius as the recording pill chips and `Chip`), 11 px, weight 500. Padding: left 1 px, right 7 px. The image thumbnail is inset 2 px on every side (1 px transparent border + 1 px padding horizontally, centred in the 18 px content height vertically); its radius 5 + 2 = 7, so its corners are concentric with the chip. The accent fill marks the chat Context bar's "has context" state instead of an accent edge. The recording pill keeps grey chips.
- Used by the chat Context header (`conversation-dialog/ContextSection.svelte`) and the context menu Context section (`context-menu/ContextSection.svelte`). Not used by the composer and `ContextEditor` (they keep `ImageChipBar` / `AttachmentChip`); the recording pill has its own markup.

### ImageChipBar

- `images` is bindable; `onremove` overrides the default removal from the array; `onopen` is called when a thumbnail is opened.
- Renders nothing when `images` is empty.
- Chip label is `Image #{image.number ?? position + 1}`: the optional `number` of `ConversationImage` wins, the 1-based position is the fallback.
- Each chip is an `AttachmentChip` with a thumbnail from the base64 data URI (120 px, or 64 px with `variant="small"`). Delete removes the image from the bound array unless `onremove` is given.
- Import `ConversationImage` from `$lib/types/conversation`.

### ContextEditor

Lives in `shared/widgets/ContextEditor.svelte` (it calls `openImagePreview` from `$lib/services/windowPreviews` for `ImageChipBar`'s `onopen`), not here. Parents control persistence.

### CommandPalette

- `inputRef` is bindable so the consumer can call `.focus()`.
- Pure visual shell: shared frame (`var(--surface-floating-modal)` with `--surface-floating-modal-border`), input row, scrollable body, footer. Consumers render the list and footer keys via snippets.
- `variant="overlay"` is for in-window overlays (e.g. Cmd+K inside ConversationDialog) — adds the `--surface-scrim` backdrop with fade-in.
- `variant="window"` is for separate Tauri windows with transparent body (e.g. Cmd+P provider switcher) — keeps the scrim transparent so the host window itself acts as the backdrop. On Linux this variant uses a lighter shadow (`--shadow-lg-linux`) because GTK transparent windows render heavy shadows poorly; the overlay variant keeps `--shadow-lg` on every platform.
- Provides utility classes for consumers: `.palette-item`, `.palette-item.highlight`, `.palette-item-icon`, `.palette-item-name`, `.palette-empty`, `.palette-divider`, plus `.palette-footer kbd` styling.

### MarkdownRenderer

- `onopen` and `onsavesvg` are optional callbacks (open a link, save a Mermaid SVG).
- Uses `marked` + `highlight.js` via `$lib/utils/markdown.ts` to render markdown with syntax-highlighted code blocks.
- `renderMarkdown` always highlights code blocks (`highlight.js`, auto-detecting the language when none is given); the renderer's `isStreaming` only paces how much of `content` is shown, not highlighting.
- Tables are styled here for every consumer. The `table` renderer of `markdown.ts` wraps each in `div.table-wrapper`, which carries the frame and the horizontal scroll. The table spans 100 % of the column with auto layout, so its width stays constant while a reply streams. The first column is nowrap.
- Code copy: each code block gets a "Copy" button with a `data-copy-index` attribute. Click handler uses event delegation on the container div to copy the raw code block content to clipboard.
- Copy feedback: the code-block Copy button and, after "Copy as SVG", the mermaid dots button show `icons.Check` for `COPY_FEEDBACK_MS` through one timer per button (a repeated click restarts it), then the fixed icon is restored. This `innerHTML` swap is the one copy feedback outside `useCopyFeedback`, because the buttons are HTML strings from `markdown.ts`.

