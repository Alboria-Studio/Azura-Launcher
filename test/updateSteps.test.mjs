import assert from "node:assert/strict";
import { test } from "node:test";

import { stepIndex, UPDATE_STEPS, updateBarValue } from "../src/updateSteps.js";

test("the update window's steps follow the updater's phases, in order", () => {
  assert.deepEqual(UPDATE_STEPS, ["download", "verify", "install", "restart", "game"]);
  assert.equal(stepIndex("install"), 2);
  // Anything unknown shows as the first step.
  assert.equal(stepIndex("later"), 0);
});

test("the bar moves through the download and the game's build, and is busy while they are unknown", () => {
  assert.equal(updateBarValue("download", 0, null), 0);
  assert.equal(updateBarValue("download", 50, null), 10);
  assert.equal(updateBarValue("download", null, null), null);
  // The short steps fill their fifth at once.
  assert.equal(updateBarValue("verify", 100, null), 40);
  assert.equal(updateBarValue("restart", 100, null), 80);
  assert.equal(updateBarValue("game", 100, null), null);
  assert.equal(updateBarValue("game", 100, 50), 90);
  assert.equal(updateBarValue("game", 100, 100), 100);
});

test("the start's check is no step: the bar is busy until a version is found", () => {
  assert.equal(updateBarValue("check", null, null), null);
  assert.equal(updateBarValue("check", 100, 100), null);
});
