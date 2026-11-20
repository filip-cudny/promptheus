import test from "node:test";
import assert from "node:assert";
import {
  menuWindowHeight,
  closedMenuHeight,
  closedOffset,
  offsetAfterToggle,
  menuInputRect,
} from "../src/lib/utils/menuLayout.ts";

test("the window height is the closed menu plus the list block", () => {
  assert.strictEqual(menuWindowHeight(456, 194), 650);
});

test("the closed menu height drops the list block only while the list is open", () => {
  assert.strictEqual(closedMenuHeight(650, true, 194), 456);
  assert.strictEqual(closedMenuHeight(456, false, 194), 456);
});

test("the closed offset is the shift the clamp applied, within the list block", () => {
  assert.strictEqual(closedOffset(659, 659, 194), 0);
  assert.strictEqual(closedOffset(984, 790, 194), 194);
  assert.strictEqual(closedOffset(900, 790, 194), 110);
  assert.strictEqual(closedOffset(1100, 790, 194), 194);
  assert.strictEqual(closedOffset(20, 32, 194), 0);
});

test("an expand resets the offset, a collapse keeps it", () => {
  assert.strictEqual(offsetAfterToggle(194, true), 0);
  assert.strictEqual(offsetAfterToggle(0, false), 0);
});

test("the input rect bounds the menu and every panel", () => {
  assert.deepStrictEqual(menuInputRect(194, 456, 320, []), { x: 0, y: 194, width: 320, height: 456 });
  assert.deepStrictEqual(
    menuInputRect(194, 456, 320, [{ top: 100, bottom: 300 }]),
    { x: 0, y: 100, width: 320, height: 550 },
  );
  assert.deepStrictEqual(
    menuInputRect(0, 456, 320, [{ top: 400, bottom: 600 }]),
    { x: 0, y: 0, width: 320, height: 600 },
  );
});
