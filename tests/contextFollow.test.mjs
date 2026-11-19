import test from "node:test";
import assert from "node:assert";
import {
  acceptsGlobalResult,
  applyGlobalChange,
  contextDiffers,
  editContext,
  followGlobal,
  keepOwnVersion,
  restoreOwnVersion,
  syncFollowingTabs,
} from "../src/lib/utils/contextFollow.ts";

const IMG1 = { data: "AAA", media_type: "image/png", number: 1 };
const TEXT1 = '<pasted-text name="Text #1">\nSprawdziłem zgłoszone CVE dotyczące\n</pasted-text>';

const tabWith = (overrides = {}) => ({
  context_text: "",
  context_images: [],
  context_own: false,
  context_hint: false,
  ...overrides,
});

test("following tab with a screenshot takes a text-only global context", () => {
  const tab = tabWith({ context_images: [IMG1] });
  applyGlobalChange([tab], { text: TEXT1, images: [] });
  assert.strictEqual(tab.context_text, TEXT1);
  assert.deepStrictEqual(tab.context_images, []);
  assert.strictEqual(tab.context_own, false);
  assert.strictEqual(tab.context_hint, false);
});

test("every following tab in the list takes the new global context", () => {
  const active = tabWith();
  const inactive = tabWith({ context_images: [IMG1] });
  applyGlobalChange([active, inactive], { text: TEXT1, images: [] });
  for (const tab of [active, inactive]) {
    assert.strictEqual(tab.context_text, TEXT1);
    assert.deepStrictEqual(tab.context_images, []);
  }
});

test("own-version tab keeps its context and gets a hint only when global differs", () => {
  const tab = tabWith({ context_images: [IMG1], context_own: true });
  applyGlobalChange([tab], { text: TEXT1, images: [] });
  assert.strictEqual(tab.context_text, "");
  assert.deepStrictEqual(tab.context_images, [IMG1]);
  assert.strictEqual(tab.context_own, true);
  assert.strictEqual(tab.context_hint, true);

  applyGlobalChange([tab], { text: "", images: [{ ...IMG1 }] });
  assert.strictEqual(tab.context_hint, false);
});

test("editContext marks own and keeps the hint, followGlobal and keepOwnVersion reset", () => {
  const tab = tabWith({ context_hint: true });
  editContext(tab, { text: TEXT1, images: [IMG1] });
  assert.strictEqual(tab.context_own, true);
  assert.strictEqual(tab.context_hint, true);
  assert.strictEqual(tab.context_text, TEXT1);

  followGlobal(tab, { text: "global", images: [] });
  assert.strictEqual(tab.context_own, false);
  assert.strictEqual(tab.context_hint, false);
  assert.strictEqual(tab.context_text, "global");
  assert.deepStrictEqual(tab.context_images, []);

  const own = tabWith({ context_text: TEXT1, context_own: true, context_hint: true });
  keepOwnVersion(own);
  assert.strictEqual(own.context_hint, false);
  assert.strictEqual(own.context_own, true);
  assert.strictEqual(own.context_text, TEXT1);
});

test("followGlobal copies the images instead of sharing them", () => {
  const tab = tabWith();
  const global = { text: "", images: [IMG1] };
  followGlobal(tab, global);
  assert.notStrictEqual(tab.context_images, global.images);
  assert.notStrictEqual(tab.context_images[0], global.images[0]);
  assert.deepStrictEqual(tab.context_images, [IMG1]);
});

test("restoreOwnVersion sets the hint from the difference to global", () => {
  const tab = tabWith();
  restoreOwnVersion(tab, { text: TEXT1, images: [] }, { text: "other", images: [] });
  assert.strictEqual(tab.context_own, true);
  assert.strictEqual(tab.context_hint, true);
  assert.strictEqual(tab.context_text, TEXT1);

  restoreOwnVersion(tab, { text: TEXT1, images: [] }, { text: TEXT1, images: [] });
  assert.strictEqual(tab.context_own, true);
  assert.strictEqual(tab.context_hint, false);
});

test("contextDiffers compares text and images by data, media_type and number", () => {
  const base = { text: TEXT1, images: [IMG1] };
  assert.strictEqual(contextDiffers(base, { text: TEXT1, images: [{ ...IMG1 }] }), false);
  assert.strictEqual(contextDiffers(base, { text: TEXT1, images: [{ ...IMG1, number: 2 }] }), true);
  assert.strictEqual(contextDiffers(base, { text: TEXT1, images: [IMG1, IMG1] }), true);
  assert.strictEqual(contextDiffers(base, { text: "x", images: [IMG1] }), true);
  assert.strictEqual(contextDiffers(base, { text: TEXT1, images: [{ ...IMG1, data: "BBB" }] }), true);
  assert.strictEqual(contextDiffers(base, { text: TEXT1, images: [{ ...IMG1, media_type: "image/jpeg" }] }), true);
});

test("contextDiffers treats an undefined number and a null number as equal", () => {
  const withUndefined = { text: "", images: [{ data: "AAA", media_type: "image/png" }] };
  const withNull = { text: "", images: [{ data: "AAA", media_type: "image/png", number: null }] };
  assert.strictEqual(contextDiffers(withUndefined, withNull), false);
});

test("syncFollowingTabs updates following tabs and leaves own-version tabs untouched", () => {
  const following = tabWith({ context_images: [IMG1] });
  const own = tabWith({ context_text: "mine", context_own: true });
  syncFollowingTabs([following, own], { text: TEXT1, images: [] });
  assert.strictEqual(following.context_text, TEXT1);
  assert.deepStrictEqual(following.context_images, []);
  assert.strictEqual(own.context_text, "mine");
  assert.strictEqual(own.context_own, true);
  assert.strictEqual(own.context_hint, false);
});

test("Save sequence arriving out of order applies only the newest result", () => {
  const results = [
    { version: 3, value: { text: TEXT1, images: [IMG1] } },
    { version: 1, value: { text: "", images: [] } },
    { version: 2, value: { text: "", images: [IMG1] } },
  ];
  const tab = tabWith();
  let lastApplied = 0;
  for (const { version, value } of results) {
    if (!acceptsGlobalResult(lastApplied, version)) continue;
    applyGlobalChange([tab], value);
    lastApplied = version;
  }
  assert.strictEqual(lastApplied, 3);
  assert.strictEqual(tab.context_text, TEXT1);
  assert.deepStrictEqual(tab.context_images, [IMG1]);
});

test("unversioned fetch is accepted only while no newer event result was applied", () => {
  const startedAt = 4;
  assert.strictEqual(acceptsGlobalResult(4, startedAt), true);
  assert.strictEqual(acceptsGlobalResult(5, startedAt), false);
});
