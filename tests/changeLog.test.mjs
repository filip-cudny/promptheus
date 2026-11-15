import test from "node:test";
import assert from "node:assert";
import { entryTitle, conflictLines, targetLabel, effectPathLabel } from "../src/lib/utils/changeLog.ts";

const card = { id: "a", created_at: "T1", host: "h", kind: "card", operations: [{ label: "Rename model" }], undoes: null, secret: null };
const undo = { id: "b", created_at: "T2", host: "h", kind: "undo", operations: [], undoes: "a", secret: null };

test("an undo entry's title names the undone entry's time", () => {
  assert.strictEqual(entryTitle(undo, [card, undo], [], [], (t) => `at ${t}`), "Undo of at T1");
});

test("a card title lists its row labels", () => {
  assert.strictEqual(entryTitle(card, [card], [], []), "Rename model");
});

test("a conflict summary lists every target", () => {
  const lines = conflictLines([
    { row: 0, target: { type: "setting", path: "a.b" }, expected_after: "x", current: "y" },
    { row: 1, target: { type: "file", path: "skills/s/SKILL.md" }, expected_after: null, current: "z" },
  ]);
  assert.strictEqual(lines.length, 2);
  assert.ok(lines[0].includes("a.b") && lines[1].includes("skills/s/SKILL.md"));
});

test("a secret conflict names the variable and shows no values", () => {
  const lines = conflictLines([{ row: 0, target: { type: "secret", name: "OPENAI_KEY" }, expected_after: null, current: null }]);
  assert.deepStrictEqual(lines, ["OPENAI_KEY: changed since this entry"]);
});

test("a setting conflict keeps its current and expected values", () => {
  const lines = conflictLines([{ row: 0, target: { type: "setting", path: "a.b" }, expected_after: "x", current: "y" }]);
  assert.deepStrictEqual(lines, ["a.b: current y, expected x"]);
});

test("a target label is the path of a setting or file and the name of a secret", () => {
  assert.strictEqual(targetLabel({ type: "setting", path: "a.b" }), "a.b");
  assert.strictEqual(targetLabel({ type: "file", path: "skills/s/SKILL.md" }), "skills/s/SKILL.md");
  assert.strictEqual(targetLabel({ type: "secret", name: "OPENAI_KEY" }), "OPENAI_KEY");
});

const catalog = [{ key: "models[].store", section: "models", label: "Store", description: "", kind: { type: "bool" }, applies: "live" }];
const modelIds = ["m1", "m2", "m3", "m4", "m5", "m6"];
const models = modelIds.map((id) => ({ id, model: `raw-${id}`, display_name: `Model ${id}` }));
const storeOperation = (id) => ({
  op: "set_setting",
  label: `models[${id}].store`,
  effects: [{ target: { type: "setting", path: `models[${id}].store` }, before: false, after: true }],
});
const cardOf = (operations) => ({ ...card, operations });

test("a set_setting on several models titles the setting and the model count", () => {
  const entry = cardOf(modelIds.map(storeOperation));
  assert.strictEqual(entryTitle(entry, [entry], catalog, models), "Store: 6 models");
});

test("a set_setting on one model titles the model name and the setting", () => {
  const entry = cardOf([storeOperation("m2")]);
  assert.strictEqual(entryTitle(entry, [entry], catalog, models), "Model m2: Store");
});

test("a set_setting on a model that no longer exists titles the first 8 characters of its id", () => {
  const entry = cardOf([storeOperation("a7e8f9d0-1b2c-4d3e-8f5a-6b7c8d9e0f1a")]);
  assert.strictEqual(entryTitle(entry, [entry], catalog, models), "a7e8f9d0: Store");
});

test("an operation other than set_setting keeps its stored label", () => {
  const entry = cardOf([{ op: "add_model", label: "Add model X", effects: [] }]);
  assert.strictEqual(entryTitle(entry, [entry], catalog, models), "Add model X");
});

test("an effect path label names the model and the setting", () => {
  assert.strictEqual(effectPathLabel({ type: "setting", path: "models[m3].store" }, catalog, models), "Model m3: Store");
});
