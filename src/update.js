/*
 * What the launcher's update button shows, from the update found, the download's progress and
 * the game's state. Kept apart from main.js so it can be tested without a window.
 *
 * Installing an update restarts the launcher, so it waits while the game starts or runs: the
 * button stays visible but disabled, and says why.
 *
 * When an update is found decides how it is offered. One found as the launcher starts installs at
 * once, without asking (shouldForceUpdate), as the player has not started anything yet. One found
 * while the launcher is already open asks (shouldAskUpdate): "Update now" installs it, "Later"
 * leaves it until the launcher's next start, which installs it. Until a found update is installed
 * the player may still play, but not online (playsOffline): the Alboria server only takes the
 * newest version, and the game goes in system link mode instead of failing to connect.
 */

/**
 * The update button's state.
 *
 * @param {{ version: string } | null} update  The newer launcher the updater found, or null.
 * @param {number | null} progress  null before downloading, -1 while the size is unknown, else 0 to 100.
 * @param {string} game  The game's state: "idle", or anything else while it starts or runs.
 * @returns {{ hidden: boolean, disabled: boolean, key?: string, vars?: object, hint?: string }}
 *   `key` and `vars` name the button's text in the locale files, `hint` its tooltip.
 */
export const updateButtonState = (update, progress, game) => {
  if (!update) return { hidden: true, disabled: true };
  if (progress !== null) {
    return { hidden: false, disabled: true, key: "update.downloading", vars: { percent: progress } };
  }
  const busy = game !== "idle";
  return {
    hidden: false,
    disabled: busy,
    key: "update.available",
    vars: { version: update.version },
    ...(busy ? { hint: "update.closeGame" } : {}),
  };
};

/** Whether an update can start now: there is one, nothing is downloading and the game is closed. */
export const canStartUpdate = (update, progress, game) => !updateButtonState(update, progress, game).disabled;

/**
 * Whether the launcher must install the update now, without asking: it was found as the launcher
 * started, it can start, and no earlier attempt in this session failed (then the player may still
 * update from the button).
 */
export const shouldForceUpdate = (update, progress, game, failed, atStart) =>
  Boolean(atStart) && !failed && canStartUpdate(update, progress, game);

/**
 * Whether the launcher asks the player to update now: an update found while it was already open,
 * that can start, that has not failed and that the player has not put off with "Later".
 */
export const shouldAskUpdate = (update, progress, game, failed, atStart, deferred) =>
  !atStart && !failed && !deferred && canStartUpdate(update, progress, game);

/**
 * Whether Play asks first: an update was found and not installed, not failed and not put off. A
 * forced one (found at start) is installing already; the button asks about any other.
 */
export const updateBlocksPlay = (update, failed, deferred) => Boolean(update) && !failed && !deferred;

/** Whether the game starts without online: a newer launcher was found and is not installed. */
export const playsOffline = (update) => Boolean(update);
