# Styles

- `theme.css` — CSS variables. `global.css` imports `theme.css` and `fonts.css`.
- Accent or status text and icons on a tint (`--accent-bg-soft`, `--accent-bg`, `--surface-overlay*` over an accent element, `--success-bg-soft`, `--danger-bg-soft`, `--warning-bg-soft`) use `--accent-text`, `--success-text`, `--danger-text` or `--warning-text`, never `--accent`, `--success`, `--danger`, `--warning`, `--accent-vivid` or `--accent-ring`: on a tint those stay below WCAG 2.x contrast (SC 1.4.3 text 4.5:1, SC 1.4.11 icons 3:1). `--accent` and the status colours are for rails, borders, fills, focus rings and text on a plain surface. A primary button fill is `--button-primary-bg` / `--button-primary-bg-hover`, not `--accent`.
- `tests/themeContrast.test.mjs` checks every text-on-tint pair in both themes against those thresholds; a new pair (a new token on a tint, or a token on a new background) gets a row there.
- Font stacks (`--font-sans`, `--font-mono` in `theme.css`):
  - Default stack: Apple names first (`-apple-system, BlinkMacSystemFont`; `ui-monospace, SFMono-Regular, Menlo`), then the bundled family, so macOS keeps SF Pro and SF Mono.
  - `:root[data-platform="linux"]`: the bundled family first (`"Inter"`, `"JetBrains Mono"`), because WebKitGTK may resolve `-apple-system` to the fontconfig system font.
- `data-platform="linux"` is set by `$lib/utils/platformAttribute`, imported first by every `src/windows/*/entry.ts` (Tauri's init script sets it in the app, the import also covers pages without Tauri).
- `fonts.css` — `@font-face` rules for `"Inter"` and `"JetBrains Mono"`, `font-display: block`, `unicode-range` and weight ranges as in the Fontsource CSS.
- Font files: `src/assets/fonts/<family>/`, Fontsource variable `wght` `woff2` (`@fontsource-variable/inter` and `@fontsource-variable/jetbrains-mono`, both 5.3.0), latin and latin-ext, upright and italic, with `OFL.txt` (SIL Open Font License 1.1) from the package. To update, unpack the new npm tarballs and copy the same files; compare `unicode-range` and weights with the package's `wght.css` and `wght-italic.css`.
- `src/` sets `font-family` only through `var(--font-sans)`, `var(--font-mono)` or `inherit` (the `@font-face` rules in `fonts.css` excepted).
- A third-party component that sets its own `font-family` is overridden with `var(--font-sans)` in the component that wraps it (`NotificationStack.svelte` for `svelte-sonner`).
