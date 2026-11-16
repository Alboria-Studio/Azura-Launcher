/*
 * The update window (update.html, src-tauri/src/updater.rs): what an update is doing while the
 * launcher is closed. It reads the update under way (update_state) and follows its events: the
 * download's progress, each step, and the game's build in the new launcher. The main window ends it
 * (finish_update) once the game is ready; this window only shows.
 */

import { initI18n, t, translatePage } from "./i18n.js";
import { loadSettings } from "./platform.js";
import { stepIndex, UPDATE_STEPS, updateBarValue } from "./updateSteps.js";

const tauri = globalThis.__TAURI__;

/** What the window shows: the step, the download's percent and the build's (null while unknown). */
const view = { phase: "download", download: null, build: null };

/** The steps along the bottom, done, under way or still to come. */
const renderSteps = () => {
  const now = stepIndex(view.phase);
  document.getElementById("update-steps").replaceChildren(
    ...UPDATE_STEPS.map((step, index) => {
      const item = document.createElement("li");
      item.textContent = t(`updateWindow.step.${step}`);
      if (index < now) item.className = "done";
      else if (index === now) item.className = "now";
      return item;
    }),
  );
  document.getElementById("update-step-now").textContent = t(`updateWindow.doing.${view.phase}`);
  const value = updateBarValue(view.phase, view.download, view.build);
  const bar = document.querySelector(".update-bar");
  bar.classList.toggle("busy", value === null);
  if (value !== null) {
    document.getElementById("update-fill").style.width = `${value}%`;
    bar.setAttribute("aria-valuenow", String(Math.round(value)));
  } else {
    bar.removeAttribute("aria-valuenow");
  }
};

/** The versions and the notes of the update. */
const renderUpdate = (update) => {
  document.getElementById("update-from").textContent = update.from;
  document.getElementById("update-to").textContent = update.to;
  document.getElementById("update-notes-title").textContent = t("updateWindow.notes", { version: update.to });
  const list = document.getElementById("update-notes-list");
  list.replaceChildren(
    ...update.notes.map((note) => {
      const item = document.createElement("li");
      item.textContent = note;
      return item;
    }),
  );
  document.querySelector(".update-notes").hidden = update.notes.length === 0;
};

const start = async () => {
  const settings = await loadSettings().catch(() => null);
  await initI18n(settings?.uiLanguage);
  translatePage();
  const update = await tauri.core.invoke("update_state");
  if (update) {
    renderUpdate(update);
    view.phase = update.phase;
  }
  renderSteps();
  tauri.event.listen("update-phase", (event) => {
    view.phase = event.payload;
    renderSteps();
  });
  tauri.event.listen("update-progress", ({ payload }) => {
    view.download = payload.total ? Math.min(100, (payload.downloaded / payload.total) * 100) : null;
    renderSteps();
  });
  // The game's build in the new launcher (builder.rs): its own count of steps, when it has one.
  tauri.event.listen("runtime-build-progress", ({ payload }) => {
    view.build = payload?.total ? Math.min(100, (payload.done / payload.total) * 100) : null;
    renderSteps();
  });
};

start().catch((error) => console.error("The update window could not start:", error));
