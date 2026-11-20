import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const css = readFileSync(join(import.meta.dirname, "..", "src", "styles", "theme.css"), "utf8");

function block(selector) {
  const start = css.indexOf(`${selector} {`);
  return css.slice(start, css.indexOf("}", start));
}

function declarations(text) {
  return Object.fromEntries([...text.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
}

const dark = declarations(block(":root"));
const themes = {
  dark,
  light: { ...dark, ...declarations(block(':root[data-theme="light"]')) },
};

function resolve(vars, name) {
  const value = vars[name];
  const reference = value.match(/^var\((--[\w-]+)\)$/);
  return reference ? resolve(vars, reference[1]) : value;
}

function parseColor(value) {
  const hex = value.match(/#([0-9a-f]{6}|[0-9a-f]{3})\b/i);
  if (hex) {
    const digits = hex[1].length === 3 ? [...hex[1]].map((d) => d + d) : hex[1].match(/../g);
    return [...digits.map((d) => parseInt(d, 16)), 1];
  }
  const [r, g, b, a = 1] = value.match(/rgba?\(([^)]+)\)/)[1].split(",").map(Number);
  return [r, g, b, a];
}

function over([r, g, b, a], [br, bg, bb]) {
  return [r * a + br * (1 - a), g * a + bg * (1 - a), b * a + bb * (1 - a), 1];
}

function luminance([r, g, b]) {
  const linear = (c) => (c / 255 <= 0.04045 ? c / 255 / 12.92 : ((c / 255 + 0.055) / 1.055) ** 2.4);
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

function contrast(vars, { fg, layers, surface }) {
  const color = (name) => parseColor(resolve(vars, name));
  const background = layers.reduce((under, layer) => over(color(layer), under), color(surface));
  const [l1, l2] = [luminance(over(color(fg), background)), luminance(background)].sort((a, b) => b - a);
  return (l1 + 0.05) / (l2 + 0.05);
}

const TEXT = 4.5;
const ICON = 3;

const pairs = [
  ["ToolChip icon, dismiss, rest", "--accent-text", ["--accent-bg-soft"], "--surface-base", ICON],
  ["ToolChip icon, dismiss, hover", "--accent-text", ["--accent-bg"], "--surface-base", ICON],
  ["ToolChip label fallback", "--accent-text", ["--accent-bg-soft"], "--surface-base", TEXT],
  ["ContextItemChip number, rest and hover", "--accent-text", ["--surface-overlay-faint", "--accent-bg-soft"], "--surface-base", TEXT],
  ["ContextItemChip number in the hint panel", "--accent-text", ["--accent-bg-soft"], "--surface-floating-popover", TEXT],
  ["ContextItemChip icon", "--accent-text", ["--surface-overlay-faint", "--accent-bg-soft"], "--surface-base", ICON],
  ["HighlightedTextarea placeholder", "--accent-text", ["--accent-bg-soft"], "--surface-elevated", TEXT],
  ["HighlightedTextarea unknown placeholder", "--warning-text", ["--warning-bg-soft"], "--surface-elevated", TEXT],
  ["SelectDropdown active trigger", "--accent-text", ["--accent-bg-soft"], "--surface-base", TEXT],
  ["SelectDropdown selected option, MenuItem active", "--accent-text", ["--accent-bg-soft"], "--surface-floating-popover", TEXT],
  ["MenuItem active in the context menu", "--accent-text", ["--accent-bg-soft"], "--surface-base", TEXT],
  ["SkillEditable autocomplete name", "--accent-text", ["--accent-bg-soft"], "--surface-elevated", TEXT],
  ["History segment, page size", "--accent-text", ["--accent-bg-soft"], "--surface-base", TEXT],
  ["SkillFilterPicker active, hovered", "--accent-text", ["--accent-bg-soft", "--accent-bg-soft"], "--surface-base", TEXT],
  ["ModelEditor, CreateModelDialog segmented", "--accent-text", ["--accent-bg-soft"], "--surface-sunken", TEXT],
  ["CapabilitiesEditor, SetupBanner, SkillList slug, ModelAssignments link", "--accent-text", ["--accent-bg-soft"], "--surface-base", TEXT],
  ["SkillEditor slug chip", "--accent-text", ["--surface-overlay-faint"], "--surface-base", TEXT],
  ["ImportConflictDialog code", "--accent-text", ["--surface-overlay-faint"], "--surface-floating-modal", TEXT],
  ["UserBubble skill badge", "--accent-text", ["--user-bubble-bg", "--surface-overlay"], "--surface-base", TEXT],
  ["SttKeytermsEditor chip remove, hover", "--accent-text", ["--accent-bg-soft"], "--surface-raised", ICON],
  ["SttKeytermsEditor input plus icon, focused or filled", "--accent-text", ["--surface-overlay-faint", "--accent-bg-soft"], "--surface-base", ICON],
  ["Status pill success", "--success-text", ["--success-bg-soft"], "--surface-base", TEXT],
  ["Status pill error", "--danger-text", ["--danger-bg-soft"], "--surface-base", TEXT],
  ["Status pill empty", "--warning-text", ["--warning-bg-soft"], "--surface-base", TEXT],
  ["Status pill partial", "--accent-text", ["--accent-bg-soft"], "--surface-base", TEXT],
  ["Danger text on its tint, base", "--danger-text", ["--danger-bg-soft"], "--surface-base", TEXT],
  ["Danger text on its tint, elevated", "--danger-text", ["--danger-bg-soft"], "--surface-elevated", TEXT],
  ["Success text on its tint, base", "--success-text", ["--success-bg-soft"], "--surface-base", TEXT],
  ["Success text on its tint, elevated", "--success-text", ["--success-bg-soft"], "--surface-elevated", TEXT],
  ["Context hint badge", "--warning-text", ["--warning-bg-soft"], "--surface-floating-popover", TEXT],
  ["Primary button rest", "--accent-fg", [], "--button-primary-bg", TEXT],
  ["Primary button hover", "--accent-fg", [], "--button-primary-bg-hover", TEXT],
  ["Search mark rest", "--info", [], "--surface-base", TEXT],
  ["Search mark selected row", "--info", ["--surface-overlay"], "--surface-base", TEXT],
];

for (const [theme, vars] of Object.entries(themes)) {
  for (const [place, fg, layers, surface, min] of pairs) {
    test(`${theme}: ${place} reaches ${min}:1`, () => {
      const ratio = contrast(vars, { fg, layers, surface });
      assert.ok(ratio >= min, `${fg} on ${[surface, ...layers].join(" + ")}: ${ratio.toFixed(2)}`);
    });
  }
}
