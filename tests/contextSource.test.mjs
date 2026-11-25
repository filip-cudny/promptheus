import test from "node:test";
import assert from "node:assert";
import { readFileSync } from "node:fs";
import {
  classifyLines,
  parseContextSource,
  placePaste,
  serializeContextSource,
} from "../src/lib/utils/contextSource.ts";

const L = "x".repeat(600);
const opts = { threshold: 500, plain: false };
const S = "--- text\nabc\n--- text\ndef";

test("parse: empty and whitespace-only fields have no items", () => {
  assert.deepStrictEqual(parseContextSource(""), []);
  assert.deepStrictEqual(parseContextSource("  \n"), []);
});

test("parse: text without a marker is one item", () => {
  assert.deepStrictEqual(parseContextSource("hello\nworld"), ["hello\nworld"]);
});

test("parse: the text before the first marker is the first item", () => {
  assert.deepStrictEqual(parseContextSource("intro\n--- text\nA\n--- text\nB"), ["intro", "A", "B"]);
});

test("parse: blank items are dropped", () => {
  assert.deepStrictEqual(parseContextSource("--- text\n\n--- text\nB"), ["B"]);
});

test("parse: marker lines are case and spacing tolerant", () => {
  assert.deepStrictEqual(parseContextSource("--- text\nA\n---TEXT\nB\n---  Text \nC"), ["A", "B", "C"]);
});

test("parse: a damaged marker line stays content of its item", () => {
  assert.deepStrictEqual(parseContextSource("--- text\nA\n--- te\nB"), ["A\n--- te\nB"]);
});

test("parse: a bare --- and free text after --- are content", () => {
  assert.deepStrictEqual(parseContextSource("--- text\nA\n---\n--- see above"), ["A\n---\n--- see above"]);
});

test("classifyLines: a damaged marker line reports the item it falls into", () => {
  assert.deepStrictEqual(classifyLines("--- text\nA\n--- te\nB")[2], { kind: "damaged", text: "--- te", itemNumber: 1 });
  assert.deepStrictEqual(classifyLines("--- text\nA\n--- tex\nB")[2], {
    kind: "damaged",
    text: "--- tex",
    itemNumber: 1,
  });
});

test("classifyLines: a bare --- and free text after --- are content", () => {
  const lines = classifyLines("--- text\nA\n---\n--- see above");
  assert.strictEqual(lines[2].kind, "content");
  assert.strictEqual(lines[3].kind, "content");
});

test("classifyLines: marker numbers skip blank items", () => {
  const lines = classifyLines("--- text\nA\n--- text\n\n--- text\nB");
  assert.deepStrictEqual(
    lines.map((line) => line.kind),
    ["marker", "content", "marker", "content", "marker", "content"],
  );
  assert.deepStrictEqual(
    lines.filter((line) => line.kind === "marker").map((line) => line.number),
    [1, null, 2],
  );
});

test("classifyLines: a non-blank leading item takes number 1", () => {
  assert.strictEqual(classifyLines("intro\n--- text\nA")[1].number, 2);
});

test("classifyLines: a blank leading item takes no number", () => {
  assert.strictEqual(classifyLines("\n--- text\nA")[1].number, 1);
});

test("serialize: no, one and several items", () => {
  assert.strictEqual(serializeContextSource([]), "");
  assert.strictEqual(serializeContextSource(["only"]), "only");
  assert.strictEqual(serializeContextSource(["A", "B"]), "--- text\nA\n--- text\nB");
});

test("serialize: blank items are dropped", () => {
  assert.strictEqual(serializeContextSource(["A", "  ", "B"]), "--- text\nA\n--- text\nB");
  assert.strictEqual(serializeContextSource(["", "A"]), "A");
});

test("round trip: parse of serialize gives the items back", () => {
  for (const items of [["A"], ["A", "B\nC"], ["A\n", "B"]]) {
    assert.deepStrictEqual(parseContextSource(serializeContextSource(items)), items);
  }
});

test("placePaste: empty field gets a marker before the text", () => {
  assert.deepStrictEqual(placePaste("", 0, 0, L, opts), { start: 0, end: 0, text: "--- text\n" + L });
});

test("placePaste: cursor inside an item pastes after that item", () => {
  assert.deepStrictEqual(placePaste(S, 10, 10, L, opts), { start: 13, end: 13, text: "--- text\n" + L + "\n" });
});

test("placePaste: cursor in the last item without a trailing newline adds a leading newline", () => {
  assert.deepStrictEqual(placePaste("--- text\nabc", 10, 10, L, opts), {
    start: 12,
    end: 12,
    text: "\n--- text\n" + L,
  });
  assert.deepStrictEqual(placePaste("abc", 1, 1, L, opts), { start: 3, end: 3, text: "\n--- text\n" + L });
});

test("placePaste: an empty tail line takes the paste at the cursor", () => {
  assert.deepStrictEqual(placePaste("abc\n\n", 4, 4, L, opts), { start: 4, end: 4, text: "--- text\n" + L });
});

test("placePaste: an empty line inside an item pastes after the item", () => {
  assert.deepStrictEqual(placePaste("abc\n\ndef", 4, 4, L, opts), { start: 8, end: 8, text: "\n--- text\n" + L });
});

test("placePaste: an empty line before a marker takes the paste at the cursor", () => {
  assert.deepStrictEqual(placePaste("--- text\nabc\n\n--- text\ndef", 13, 13, L, opts), {
    start: 13,
    end: 13,
    text: "--- text\n" + L,
  });
});

test("placePaste: cursor on a marker line is in the item it opens", () => {
  assert.deepStrictEqual(placePaste(S, 2, 2, L, opts), { start: 13, end: 13, text: "--- text\n" + L + "\n" });
});

test("placePaste: text below the threshold is inserted as is", () => {
  assert.deepStrictEqual(placePaste("abc", 1, 1, "xy", opts), { start: 1, end: 1, text: "xy" });
});

test("placePaste: 499 chars are plain, 500 chars create an item", () => {
  assert.deepStrictEqual(placePaste("abc", 1, 1, "y".repeat(499), opts), {
    start: 1,
    end: 1,
    text: "y".repeat(499),
  });
  assert.deepStrictEqual(placePaste("", 0, 0, "y".repeat(500), opts), {
    start: 0,
    end: 0,
    text: "--- text\n" + "y".repeat(500),
  });
});

test("placePaste: a selection is replaced by the plain text", () => {
  assert.deepStrictEqual(placePaste("abcdef", 1, 3, L, opts), { start: 1, end: 3, text: L });
});

test("placePaste: the plain flag inserts at the cursor", () => {
  assert.deepStrictEqual(placePaste("abc", 1, 1, L, { threshold: 500, plain: true }), {
    start: 1,
    end: 1,
    text: L,
  });
});

test("placePaste: a pasted text that starts with a marker gets no second marker", () => {
  const P = "--- text\n" + L;
  assert.deepStrictEqual(placePaste(S, 10, 10, P, opts), { start: 13, end: 13, text: P + "\n" });
  assert.deepStrictEqual(placePaste("", 0, 0, P, opts), { start: 0, end: 0, text: P });
});

test("placePaste: a short text starting with a marker is plain", () => {
  assert.deepStrictEqual(placePaste("abc", 1, 1, "--- text\nab", opts), { start: 1, end: 1, text: "--- text\nab" });
});

test("the module imports nothing through $lib", () => {
  const source = readFileSync(new URL("../src/lib/utils/contextSource.ts", import.meta.url), "utf8");
  assert.ok(!source.includes("$lib"));
});
