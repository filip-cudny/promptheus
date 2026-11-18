import test from "node:test";
import assert from "node:assert";
import { readFileSync } from "node:fs";
import { PASTED_TEXT_BLOCK, parseContextTextItems } from "../src/lib/utils/contextBlocks.ts";

const block = (number, content) => `<pasted-text name="Text #${number}">\n${content}\n</pasted-text>`;

test("two tagged blocks give their numbers and contents", () => {
  const items = parseContextTextItems(`${block(1, "first words here")}\n\n${block(3, "third block text")}`);
  assert.deepStrictEqual(
    items.map((item) => item.number),
    [1, 3],
  );
  assert.ok(items[0].text.startsWith("first words"));
  assert.ok(items[1].text.startsWith("third block"));
});

test("untagged text is one item without a number", () => {
  assert.deepStrictEqual(parseContextTextItems("  typed by hand \n"), [{ number: null, text: "typed by hand" }]);
});

test("tagged blocks plus text outside them: numbered items then one without a number", () => {
  const items = parseContextTextItems(`typed before\n${block(2, "pasted")}\n\n${block(4, "more")}\ntyped after`);
  assert.deepStrictEqual(
    items.map((item) => item.number),
    [2, 4, null],
  );
  assert.ok(items[2].text.includes("typed before"));
  assert.ok(items[2].text.includes("typed after"));
});

test("empty and whitespace-only strings give no items", () => {
  assert.deepStrictEqual(parseContextTextItems(""), []);
  assert.deepStrictEqual(parseContextTextItems(" \n\t"), []);
});

test("the block regex matches the format of pasted_text_block in context.rs", () => {
  const rust = readFileSync(new URL("../src-tauri/src/services/context.rs", import.meta.url), "utf8");
  assert.ok(rust.includes('format!("<pasted-text name=\\"Text #{number}\\">\\n{content}\\n</pasted-text>")'));
  assert.strictEqual([...block(7, "x").matchAll(PASTED_TEXT_BLOCK)].length, 1);
});
