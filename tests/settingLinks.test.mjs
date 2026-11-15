import test from "node:test";
import assert from "node:assert";
import { parseSectionLink, parseSettingLink, showSettingLinks } from "../src/lib/utils/settingLinks.ts";

test("setting link yields its key", () => {
  assert.strictEqual(parseSettingLink("setting:models[gpt].display_name"), "models[gpt].display_name");
});

test("other links are not setting links", () => {
  assert.strictEqual(parseSettingLink("https://x"), null);
  assert.strictEqual(parseSettingLink("setting:"), null);
});

test("setting links are relabelled show, other links stay", () => {
  assert.strictEqual(
    showSettingLinks("Theme [here](setting:theme) and [x](https://x)"),
    "Theme [show](setting:theme) and [x](https://x)",
  );
});

test("section link yields its section id", () => {
  assert.strictEqual(parseSectionLink("section:environment"), "environment");
  assert.strictEqual(parseSectionLink("section:sync"), "sync");
  assert.strictEqual(parseSectionLink("section:shortcuts"), "shortcuts");
});

test("unknown section and field keys are not section links", () => {
  assert.strictEqual(parseSectionLink("section:nope"), null);
  assert.strictEqual(parseSectionLink("theme"), null);
});

test("setting link keeps the section key", () => {
  assert.strictEqual(parseSettingLink("setting:section:environment"), "section:environment");
});
