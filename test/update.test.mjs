/*
 * Tests for the launcher's update button.
 *
 * Run with: npm test
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  canStartUpdate,
  playsOffline,
  shouldAskUpdate,
  shouldForceUpdate,
  updateBlocksPlay,
  updateButtonState,
} from "../src/update.js";

const update = { version: "0.6.0" };

test("no update: the button is hidden", () => {
  assert.deepEqual(updateButtonState(null, null, "idle"), { hidden: true, disabled: true });
  assert.equal(canStartUpdate(null, null, "idle"), false);
});

test("an update with the game closed can start", () => {
  assert.deepEqual(updateButtonState(update, null, "idle"), {
    hidden: false,
    disabled: false,
    key: "update.available",
    vars: { version: "0.6.0" },
  });
  assert.equal(canStartUpdate(update, null, "idle"), true);
});

test("while the game starts or runs the button waits and says why", () => {
  for (const game of ["starting", "running", "preparing"]) {
    const view = updateButtonState(update, null, game);
    assert.equal(view.hidden, false);
    assert.equal(view.disabled, true);
    assert.equal(view.hint, "update.closeGame");
    assert.equal(canStartUpdate(update, null, game), false);
  }
});

test("while downloading the button shows the progress and cannot start again", () => {
  assert.deepEqual(updateButtonState(update, 42, "idle"), {
    hidden: false,
    disabled: true,
    key: "update.downloading",
    vars: { percent: 42 },
  });
  assert.equal(updateButtonState(update, -1, "idle").vars.percent, -1);
  assert.equal(canStartUpdate(update, 42, "idle"), false);
});

test("an update found at start installs without asking, unless it failed", () => {
  assert.equal(shouldForceUpdate(update, null, "idle", false, true), true);
  assert.equal(shouldForceUpdate(update, null, "idle", true, true), false);
  assert.equal(shouldForceUpdate(update, 10, "idle", false, true), false);
  assert.equal(shouldForceUpdate(null, null, "idle", false, true), false);
});

test("an update found while open is never forced: it asks once it can start", () => {
  assert.equal(shouldForceUpdate(update, null, "idle", false, false), false);
  assert.equal(shouldAskUpdate(update, null, "idle", false, false, false), true);
  // While the game runs it waits, and asks when the game closes.
  assert.equal(shouldAskUpdate(update, null, "running", false, false, false), false);
  // "Later", a failed install, a download in progress or a forced update do not ask.
  assert.equal(shouldAskUpdate(update, null, "idle", false, false, true), false);
  assert.equal(shouldAskUpdate(update, null, "idle", true, false, false), false);
  assert.equal(shouldAskUpdate(update, 10, "idle", false, false, false), false);
  assert.equal(shouldAskUpdate(update, null, "idle", false, true, false), false);
  assert.equal(shouldAskUpdate(null, null, "idle", false, false, false), false);
});

test("Play asks while an update has no answer; after Later or a failure it plays offline", () => {
  assert.equal(updateBlocksPlay(update, false, false), true);
  assert.equal(updateBlocksPlay(update, false, true), false);
  assert.equal(updateBlocksPlay(update, true, false), false);
  assert.equal(updateBlocksPlay(null, false, false), false);
  assert.equal(playsOffline(update), true);
  assert.equal(playsOffline(null), false);
});
