import test from "node:test";
import assert from "node:assert/strict";
import { fuzzyMatchIndices, rankByQuery } from "../src/lib/utils/fuzzyRank.ts";

const one = (s) => [s];

test("rankByQuery puts the substring match before the scattered one", () => {
  const ranked = rankByQuery(["Draft of meeting", "renew the domain"], "dom", one);
  assert.equal(ranked[0].item, "renew the domain");
});

test("a word-start substring beats a mid-word one", () => {
  const ranked = rankByQuery(["freedom fighters", "domain transfer"], "dom", one);
  assert.equal(ranked[0].item, "domain transfer");
});

test("indices of a substring and of a scattered match", () => {
  assert.deepEqual(fuzzyMatchIndices("dom", "renew the domain").indices, [10, 11, 12]);
  assert.deepEqual(fuzzyMatchIndices("dom", "Draft of meeting").indices, [0, 6, 9]);
});

test("matching is case-insensitive", () => {
  const match = fuzzyMatchIndices("DOM", "Domain");
  assert.notEqual(match, null);
  assert.deepEqual(match.indices, [0, 1, 2]);
});

test("indices are code points", () => {
  assert.deepEqual(fuzzyMatchIndices("ab", "🎙 ab").indices, [2, 3]);
});

test("no match returns null and rankByQuery drops the item", () => {
  assert.equal(fuzzyMatchIndices("xyz", "renew the domain"), null);
  assert.deepEqual(rankByQuery(["renew the domain"], "xyz", one), []);
});

test("an empty query keeps the order", () => {
  const ranked = rankByQuery(["b", "a", "c"], "", one);
  assert.deepEqual(ranked.map((r) => r.item), ["b", "a", "c"]);
});

test("a match in the second text keeps the item", () => {
  const ranked = rankByQuery(
    [{ title: "Notes", skill: "Translate" }],
    "trans",
    (i) => [i.title, i.skill],
  );
  assert.equal(ranked.length, 1);
  assert.equal(ranked[0].matches[0], null);
  assert.notEqual(ranked[0].matches[1], null);
});
