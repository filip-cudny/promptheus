# Styles

- `theme.css` — CSS variables. `global.css` imports `theme.css` and `fonts.css`.
- Font stacks (`--font-sans`, `--font-mono` in `theme.css`):
  - Default stack: Apple names first (`-apple-system, BlinkMacSystemFont`; `ui-monospace, SFMono-Regular, Menlo`), then the bundled family, so macOS keeps SF Pro and SF Mono.
  - `:root[data-platform="linux"]`: the bundled family first (`"Inter"`, `"JetBrains Mono"`), because WebKitGTK may resolve `-apple-system` to the fontconfig system font.
- `data-platform="linux"` is set by `$lib/utils/platformAttribute`, imported first by every `src/windows/*/entry.ts` (Tauri's init script sets it in the app, the import also covers pages without Tauri).
- `fonts.css` — `@font-face` rules for `"Inter"` and `"JetBrains Mono"`, `font-display: block`, `unicode-range` and weight ranges as in the Fontsource CSS.
- Font files: `src/assets/fonts/<family>/`, Fontsource variable `wght` `woff2` (`@fontsource-variable/inter` and `@fontsource-variable/jetbrains-mono`, both 5.3.0), latin and latin-ext, upright and italic, with `OFL.txt` (SIL Open Font License 1.1) from the package. To update, unpack the new npm tarballs and copy the same files; compare `unicode-range` and weights with the package's `wght.css` and `wght-italic.css`.
- `src/` sets `font-family` only through `var(--font-sans)`, `var(--font-mono)` or `inherit` (the `@font-face` rules in `fonts.css` excepted).
