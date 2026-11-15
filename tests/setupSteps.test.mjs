import test from "node:test";
import assert from "node:assert";
import {
  createdModelPassed,
  setupStep,
  testNote,
  testableModel,
} from "../src/lib/utils/setupSteps.ts";

const textModel = { id: "t1", type: "text", api_key: "${OPENAI_API_KEY}" };
const literalModel = { id: "t2", type: "text", api_key: "sk-literal" };
const sttModel = { id: "s1", type: "stt", api_key: "${ELEVENLABS_API_KEY}" };

function surfaces({ agent = null, chat = null, speech = null } = {}) {
  return {
    chat: { generation: { model_id: chat } },
    settings_agent: { generation: { model_id: agent } },
    speech_to_text: { model_id: speech },
  };
}

function inputs(overrides = {}) {
  return {
    surfaces: surfaces(),
    models: [],
    isEnvSet: () => false,
    syncState: "off",
    textPassed: false,
    speechPassed: false,
    contextMenuShortcut: null,
    ...overrides,
  };
}

test("step 2 without models asks for a text model", () => {
  const step = setupStep(2, inputs());
  assert.strictEqual(step.title, "Add a text model");
  assert.strictEqual(
    step.body,
    "Everything starts with a text model: pick a provider, paste its API key and test it.",
  );
  assert.strictEqual(step.navigate, "models:new");
  assert.deepStrictEqual(step.highlight, []);
  assert.strictEqual(step.optional, false);
});

test("step 2 with a text model whose key variable is unset names the variable", () => {
  const step = setupStep(
    2,
    inputs({ models: [textModel], surfaces: surfaces({ chat: "t1" }) }),
  );
  assert.strictEqual(step.title, "Add the API key for your models");
  assert.strictEqual(
    step.body,
    "Your repository brought the models; API keys are not synced. Paste the key on this machine.",
  );
  assert.strictEqual(step.navigate, "env:OPENAI_API_KEY");
  assert.deepStrictEqual(step.highlight, ["env-var-OPENAI_API_KEY"]);
});

test("step 2 with the key variable set or a literal key keeps the text model variant", () => {
  const set = setupStep(
    2,
    inputs({
      models: [textModel],
      surfaces: surfaces({ agent: "t1" }),
      isEnvSet: (name) => name === "OPENAI_API_KEY",
    }),
  );
  assert.strictEqual(set.title, "Add a text model");
  const literal = setupStep(
    2,
    inputs({ models: [literalModel], surfaces: surfaces({ agent: "t2" }) }),
  );
  assert.strictEqual(literal.title, "Add a text model");
});

test("step 1 is done when the sync state is not off", () => {
  assert.strictEqual(setupStep(1, inputs({ syncState: "off" })).done, false);
  assert.strictEqual(setupStep(1, inputs({ syncState: null })).done, false);
  for (const state of ["synced", "syncing", "conflict", "error"]) {
    assert.strictEqual(setupStep(1, inputs({ syncState: state })).done, true);
  }
});

test("step 1 navigates to Sync and highlights the connect button", () => {
  const step = setupStep(1, inputs());
  assert.strictEqual(step.title, "Bring your settings from a git repository");
  assert.strictEqual(step.navigate, "section:sync");
  assert.deepStrictEqual(step.highlight, ["sync-connect"]);
  assert.strictEqual(step.optional, true);
});

test("steps 2 and 3 are done after their test passed", () => {
  assert.strictEqual(setupStep(2, inputs()).done, false);
  assert.strictEqual(setupStep(2, inputs({ textPassed: true })).done, true);
  assert.strictEqual(setupStep(2, inputs({ speechPassed: true })).done, false);
  assert.strictEqual(setupStep(3, inputs()).done, false);
  assert.strictEqual(setupStep(3, inputs({ speechPassed: true })).done, true);
  assert.strictEqual(setupStep(3, inputs({ textPassed: true })).done, false);
});

test("Next is enabled only on a done step", () => {
  assert.strictEqual(setupStep(2, inputs()).canNext, false);
  assert.strictEqual(setupStep(2, inputs({ textPassed: true })).canNext, true);
  assert.strictEqual(setupStep(4, inputs()).canNext, true);
});

test("Skip shows on steps 1 and 3 only while not done", () => {
  assert.strictEqual(setupStep(1, inputs()).showSkip, true);
  assert.strictEqual(setupStep(1, inputs({ syncState: "synced" })).showSkip, false);
  assert.strictEqual(setupStep(3, inputs()).showSkip, true);
  assert.strictEqual(setupStep(3, inputs({ speechPassed: true })).showSkip, false);
  for (const n of [2, 4, 5]) assert.strictEqual(setupStep(n, inputs()).showSkip, false);
});

test("Finish with the agent shows on steps 3 to 5 only", () => {
  for (const n of [1, 2]) assert.strictEqual(setupStep(n, inputs()).showAgent, false);
  for (const n of [3, 4, 5]) assert.strictEqual(setupStep(n, inputs()).showAgent, true);
});

test("only the last step shows Finish", () => {
  for (const n of [1, 2, 3, 4]) assert.strictEqual(setupStep(n, inputs()).isLast, false);
  assert.strictEqual(setupStep(5, inputs()).isLast, true);
});

test("step 3 navigates to a new speech model", () => {
  const step = setupStep(3, inputs());
  assert.strictEqual(step.title, "Add a speech model");
  assert.strictEqual(step.navigate, "models:new-stt");
});

test("step 4 highlights both shortcuts", () => {
  const step = setupStep(4, inputs());
  assert.strictEqual(step.title, "Choose your shortcuts");
  assert.strictEqual(step.navigate, "section:shortcuts");
  assert.deepStrictEqual(step.highlight, [
    "keymaps.open_context_menu",
    "keymaps.speech_to_text_toggle",
  ]);
});

test("step 5 text uses the context menu shortcut when there is one", () => {
  const withShortcut = setupStep(5, inputs({ contextMenuShortcut: "Ctrl+Shift+Space" }));
  assert.strictEqual(
    withShortcut.body,
    "Select text in any app and press Ctrl+Shift+Space.",
  );
  assert.deepStrictEqual(withShortcut.highlight, []);
  const without = setupStep(5, inputs());
  assert.strictEqual(
    without.body,
    "Select text in any app and press your context menu shortcut.",
  );
  assert.strictEqual(without.title, "You're set");
});

test("the Test button targets the effective text model on step 2 and the speech model on step 3", () => {
  const models = [textModel, sttModel];
  const s = surfaces({ chat: "t1", speech: "s1" });
  assert.strictEqual(testableModel(2, s, models), textModel);
  assert.strictEqual(testableModel(3, s, models), sttModel);
  assert.strictEqual(testableModel(1, s, models), null);
  assert.strictEqual(testableModel(2, surfaces(), models), null);
  assert.strictEqual(testableModel(3, surfaces({ speech: "t1" }), models), null);
});

test("a created model passes only when its test passed and it is the assigned model", () => {
  assert.strictEqual(createdModelPassed("text", "t1", true, surfaces({ chat: "t1" })), true);
  assert.strictEqual(createdModelPassed("text", "t1", false, surfaces({ chat: "t1" })), false);
  assert.strictEqual(createdModelPassed("text", "t1", true, surfaces({ chat: "other" })), false);
  assert.strictEqual(createdModelPassed("stt", "s1", true, surfaces({ speech: "s1" })), true);
  assert.strictEqual(createdModelPassed("stt", "s1", true, surfaces({ chat: "s1" })), false);
});

test("the test note shows the time or the error", () => {
  assert.strictEqual(testNote({ ok: true, millis: 412, error: null }), "OK · 412 ms");
  assert.strictEqual(testNote({ ok: false, millis: null, error: "401 Unauthorized" }), "401 Unauthorized");
});

test("an out-of-range step is clamped", () => {
  assert.strictEqual(setupStep(0, inputs()).step, 1);
  assert.strictEqual(setupStep(9, inputs()).step, 5);
});
