import test from "node:test";
import assert from "node:assert";
import { addCaptured } from "../src/lib/stores/captureInput.ts";

const first = { data: "AAA", media_type: "image/png" };
const second = { data: "BBB", media_type: "image/png" };

function makeTab() {
  return { input_images: [first], input_text_attachments: ["one"] };
}

test("an image goes to input_images as a new array keeping the earlier items", () => {
  const tab = makeTab();
  const before = tab.input_images;
  assert.strictEqual(addCaptured(tab, { kind: "image", image: second }), true);
  assert.deepStrictEqual(tab.input_images, [first, second]);
  assert.notStrictEqual(tab.input_images, before);
  assert.deepStrictEqual(tab.input_text_attachments, ["one"]);
});

test("a text goes to input_text_attachments as a new array keeping the earlier items", () => {
  const tab = makeTab();
  const before = tab.input_text_attachments;
  assert.strictEqual(addCaptured(tab, { kind: "text", text: "two" }), true);
  assert.deepStrictEqual(tab.input_text_attachments, ["one", "two"]);
  assert.notStrictEqual(tab.input_text_attachments, before);
  assert.deepStrictEqual(tab.input_images, [first]);
});

test("another tab object is not changed", () => {
  const tab = makeTab();
  const other = makeTab();
  addCaptured(tab, { kind: "image", image: second });
  addCaptured(tab, { kind: "text", text: "two" });
  assert.deepStrictEqual(other, makeTab());
});

test("a closed tab returns false", () => {
  assert.strictEqual(addCaptured(undefined, { kind: "text", text: "two" }), false);
});
