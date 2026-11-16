/*
 * The steps of a launcher update as the update window shows them (updateWindow.js), kept apart so
 * they can be tested without a window. They follow UpdatePhase in src-tauri/src/updater.rs.
 */

/** The steps, in order. */
export const UPDATE_STEPS = ["download", "verify", "install", "restart", "game"];

/** The position of `phase` among the steps; the first one for anything unknown. */
export const stepIndex = (phase) => Math.max(0, UPDATE_STEPS.indexOf(phase));

/**
 * How far the whole update has gone, 0 to 100, for the window's bar: each step is a fifth of it,
 * and the download and the game's build move within theirs as they report progress. Null while the
 * step under way has no measure (the bar then shows it is busy).
 *
 * @param {string} phase  The step under way.
 * @param {number | null} download  The download's percent, null while unknown.
 * @param {number | null} build  The game's build percent, null while unknown.
 */
export const updateBarValue = (phase, download, build) => {
  const share = 100 / UPDATE_STEPS.length;
  const index = stepIndex(phase);
  const base = index * share;
  if (phase === "download") return download === null ? null : base + (download / 100) * share;
  if (phase === "game") return build === null ? null : base + (build / 100) * share;
  // The short steps fill their share at once: they take a second or two.
  return base + share;
};
