import test from "node:test";
import assert from "node:assert";
import { readFileSync } from "node:fs";
import {
  CROSS_LINKS,
  envLink,
  parseAssignmentLink,
  parseEnvLink,
} from "../src/lib/utils/settingsCrossLinks.ts";
import { parseSectionLink } from "../src/lib/utils/settingLinks.ts";

const catalogSource = readFileSync(
  new URL("../src-tauri/src/services/config/catalog.rs", import.meta.url),
  "utf8",
);
const catalogKeys = new Set(
  [...catalogSource.matchAll(/(?:live|restart)\("([^"]+)"/g)].map((m) => m[1]),
);

test("catalog keys are collected from catalog.rs", () => {
  assert.ok(catalogKeys.size > 0);
});

test("every cross-link target resolves", () => {
  for (const [name, target] of Object.entries(CROSS_LINKS)) {
    if (target.startsWith("section:")) {
      assert.ok(parseSectionLink(target), `${name}: ${target} is not a section`);
    } else if (target.startsWith("assignment:")) {
      assert.ok(parseAssignmentLink(target), `${name}: ${target} is not an assignment`);
    } else {
      assert.ok(catalogKeys.has(target), `${name}: ${target} is not in the catalog`);
    }
  }
});

test("assignment link yields its surface", () => {
  assert.strictEqual(parseAssignmentLink("assignment:chat"), "chat");
  assert.strictEqual(parseAssignmentLink("assignment:speech_to_text"), "speech_to_text");
});

test("unknown surface and other keys are not assignment links", () => {
  assert.strictEqual(parseAssignmentLink("assignment:nope"), null);
  assert.strictEqual(parseAssignmentLink("chat"), null);
});

test("env link round-trips its name", () => {
  assert.strictEqual(envLink("OPENAI_API_KEY"), "env:OPENAI_API_KEY");
  assert.strictEqual(parseEnvLink("env:OPENAI_API_KEY"), "OPENAI_API_KEY");
});

test("empty name and other keys are not env links", () => {
  assert.strictEqual(parseEnvLink("env:"), null);
  assert.strictEqual(parseEnvLink("section:environment"), null);
});
