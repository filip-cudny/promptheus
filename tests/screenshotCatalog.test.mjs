import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseCatalog } from "../scripts/screenshots/catalog.mjs";

const source = readFileSync(
  join(import.meta.dirname, "..", "src-tauri", "src", "services", "config", "catalog.rs"),
  "utf8",
);
const catalog = parseCatalog(source);
const entry = (key) => catalog.find((e) => e.key === key);

function entryLines(text) {
  const lines = text.split("\n");
  const start = lines.findIndex((line) => line.startsWith("pub const ENTRIES"));
  const end = lines.findIndex((line, i) => i > start && line.trim() === "];");
  return lines.slice(start + 1, end).filter((line) => /^\s*(live|restart)\(/.test(line));
}

test("parses every entry line of ENTRIES", () => {
  assert.equal(catalog.length, entryLines(source).length);
});

test("restart entry with a bool kind", () => {
  const { section, kind, applies, label } = entry("show_tray_icon");
  assert.deepEqual(
    { section, kind, applies, label },
    { section: "general", kind: { type: "bool" }, applies: "after_restart", label: "Show tray icon" },
  );
});

test("SKIN constant resolves to its choice values", () => {
  assert.deepEqual(entry("appearance.logo").kind, { type: "choice", values: ["color", "mono"] });
});

test("int helper resolves to an integer kind", () => {
  assert.deepEqual(entry("history_retention_days").kind, { type: "integer", min: 0, max: 3650 });
});

test("inline text kind keeps its max length", () => {
  const { section, kind } = entry("preferred_name");
  assert.equal(section, "prompt_base");
  assert.deepEqual(kind, { type: "text", max_len: 60 });
});

test("STT_MODEL constant resolves to a model kind", () => {
  assert.deepEqual(entry("surfaces.speech_to_text.model_id").kind, { type: "model", model_type: "stt" });
});

test("an unknown entry line throws", () => {
  const broken = source.replace(entryLines(source)[0], '    bogus("x"),');
  assert.throws(() => parseCatalog(broken), Error);
});
