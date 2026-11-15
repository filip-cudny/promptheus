import test from "node:test";
import assert from "node:assert/strict";
import { buildNewModel, modelListUrl, newModelErrors } from "../src/lib/utils/newModel.ts";

const complete = {
  type: "text",
  display_name: "GPT",
  provider: "openai",
  model: "gpt-4.1-mini",
  api_key: "sk-test",
  base_url: "",
};

test("empty model is an error", () => {
  assert.ok("model" in newModelErrors({ ...complete, model: "" }));
});

test("empty api_key is an error", () => {
  assert.ok("api_key" in newModelErrors({ ...complete, api_key: "" }));
});

test("env reference api_key is accepted", () => {
  assert.ok(!("api_key" in newModelErrors({ ...complete, api_key: "${OPENAI_API_KEY}" })));
});

test("base_url with another scheme is an error", () => {
  assert.ok("base_url" in newModelErrors({ ...complete, base_url: "ftp://x" }));
});

test("complete draft has no errors", () => {
  assert.deepEqual(newModelErrors(complete), {});
});

test("buildNewModel keeps the id and stores by default", () => {
  const model = buildNewModel(complete, "id-1");
  assert.equal(model.id, "id-1");
  assert.equal(model.store, true);
});

test("modelListUrl links each provider to its model list", () => {
  assert.equal(modelListUrl("openai"), "https://platform.openai.com/docs/models");
  assert.equal(
    modelListUrl("anthropic"),
    "https://docs.claude.com/en/docs/about-claude/models/overview",
  );
  assert.equal(modelListUrl("gemini"), "https://ai.google.dev/gemini-api/docs/models");
  assert.equal(modelListUrl("elevenlabs"), "https://elevenlabs.io/docs/models");
});

test("modelListUrl is null without a provider", () => {
  assert.equal(modelListUrl(null), null);
});
