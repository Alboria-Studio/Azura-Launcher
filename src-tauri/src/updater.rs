//! Launcher self-update, on top of Tauri's updater plugin.
//!
//! The plugin reads `latest.json` from the endpoint in `tauri.conf.json`, compares its version with
//! the running one and, when newer, downloads the update archive and checks its signature against
//! the public key in the config before replacing the app. `scripts/release-launcher.sh` and
//! `scripts/publish-launcher.sh` produce both files.
//!
//! How an update looks (alboria-notes: launcher-updates.md, the same for every Alboria launcher):
//! the interface looks for one every few minutes and when the window comes to the front, and starts
//! it on its own once the game is closed. The main window then closes and a small window of its own
//! (`update.html`, 560 × 400, not resizable) shows the steps: download, signature, install, restart
//! and, in the new launcher, preparing the game from the new player kit. A file in the app's data
//! folder (`update-pending.json`) carries the update across the restart; the new launcher opens the
//! update window again at once, the interface rebuilds the game when it needs to, and
//! `finish_update` closes the window and shows the main one.
//!
//! The game runtime is not downloaded: it is built on the player's machine from their own copy of
//! the game. A new launcher brings a new player kit, and the builder then rebuilds or relinks the
//! runtime from it (`builder.rs`).

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_updater::UpdaterExt;

use crate::release_notes;

/// Event carrying the download progress of an update.
pub const PROGRESS_EVENT: &str = "update-progress";
/// Event carrying the step an update is at (`UpdatePhase`).
pub const PHASE_EVENT: &str = "update-phase";
/// The update window's label and page.
const WINDOW: &str = "update";
const PAGE: &str = "update.html";
/// Its fixed size, in logical pixels.
const WINDOW_SIZE: (f64, f64) = (560.0, 400.0);
/// How long the start's check may keep the main window hidden at most: if the interface never ends
/// it (it failed to load), the launcher shows itself anyway.
const CHECK_GIVE_UP: std::time::Duration = std::time::Duration::from_secs(45);
/// The file that carries an update across the launcher's restart.
const PENDING_FILE: &str = "update-pending.json";
/// Development only: a version to pretend is published, to try the update window
/// (`AZURA_SIMULATE_UPDATE=0.7.0`). Nothing is downloaded or installed.
const SIMULATE_ENV: &str = "AZURA_SIMULATE_UPDATE";

/// A newer launcher version, sent to the interface as JSON.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    /// The notes' list items (`release_notes::list_items`), for the question's "What's new".
    pub items: Vec<String>,
}

impl UpdateInfo {
    fn new(version: String, current_version: String, notes: Option<String>) -> Self {
        let items = release_notes::list_items(notes.as_deref().unwrap_or(""));
        Self { version, current_version, notes, items }
    }
}

/// Bytes downloaded so far and the total, when the server reports it.
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// The steps of an update, in order, as the update window names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePhase {
    /// As the launcher starts: the window says it is looking for a newer version, and the main
    /// window waits hidden until the look ends (`end_check`).
    Check,
    Download,
    Verify,
    Install,
    Restart,
    /// In the new launcher: the game is prepared from the new player kit.
    Game,
}

/// An update under way, from the version that had it to the one it brings, with its notes (the
/// list items of its changelog section) and the step it is at.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingUpdate {
    pub from: String,
    pub to: String,
    pub notes: Vec<String>,
    pub phase: UpdatePhase,
}

/// The update the update window shows, if any.
#[derive(Default)]
pub struct UpdateState(Mutex<Option<PendingUpdate>>);

impl UpdateState {
    pub fn get(&self) -> Option<PendingUpdate> {
        self.0.lock().ok().and_then(|pending| pending.clone())
    }

    fn set(&self, value: Option<PendingUpdate>) {
        if let Ok(mut pending) = self.0.lock() {
            *pending = value;
        }
    }
}

/// The version to pretend is published, in a development launcher with AZURA_SIMULATE_UPDATE set.
fn simulated_version() -> Option<String> {
    if !cfg!(feature = "dev") {
        return None;
    }
    std::env::var(SIMULATE_ENV).ok().filter(|version| !version.trim().is_empty())
}

fn pending_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join(PENDING_FILE))
}

/// Asks the update endpoint for a newer version. `Ok(None)` means the launcher is up to date. An
/// unreachable endpoint (offline) is an `Err`, which the interface treats as "no update" without
/// bothering the user.
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    if let Some(version) = simulated_version() {
        // Once the simulated update "installed", the launcher is up to date.
        if app.state::<UpdateState>().get().is_some() || pending_file(app).is_ok_and(|file| file.exists()) {
            return Ok(None);
        }
        return Ok(Some(UpdateInfo::new(
            version,
            app.package_info().version.to_string(),
            Some("- A simulated update.\n- Nothing is downloaded or installed.".to_owned()),
        )));
    }
    // The development launcher never updates itself: an update would turn it into the published one.
    if cfg!(feature = "dev") {
        return Ok(None);
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    Ok(update.map(|update| UpdateInfo::new(update.version.clone(), update.current_version.clone(), update.body.clone())))
}

/// Opens the update window, or brings it back, and hides the main window: to the player, the
/// launcher closes and the update takes its place.
pub fn open_window(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW) {
        window.show().map_err(|e| e.to_string())?;
        let _ = window.set_focus();
    } else {
        let (width, height) = WINDOW_SIZE;
        WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App(PAGE.into()))
            .title("Azura")
            .inner_size(width, height)
            .resizable(false)
            .maximizable(false)
            .center()
            .theme(Some(tauri::Theme::Dark))
            .focused(true)
            .build()
            .map_err(|e| e.to_string())?;
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    Ok(())
}

/// Closes the update window and shows the main one again.
fn close_window(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.unminimize();
        let _ = main.set_focus();
    }
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.close();
    }
}

/// Moves the update to `phase` and tells the windows.
fn enter(app: &AppHandle, pending: &mut PendingUpdate, phase: UpdatePhase) {
    pending.phase = phase;
    app.state::<UpdateState>().set(Some(pending.clone()));
    let _ = app.emit(PHASE_EVENT, phase);
}

/// Writes what the new launcher needs to carry on (it prepares the game, then shows itself) and
/// restarts into it.
fn restart_into(app: &AppHandle, pending: &PendingUpdate) -> Result<(), String> {
    let mut carried = pending.clone();
    carried.phase = UpdatePhase::Game;
    let file = pending_file(app)?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&file, serde_json::to_vec(&carried).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", file.display()))?;
    app.restart();
}

/// Downloads and installs the newer version in the update window, then restarts the launcher into
/// it. Only returns when something failed: the update window then closes and the main one is back.
pub async fn install(app: AppHandle) -> Result<(), String> {
    let result = run_install(&app).await;
    if result.is_err() {
        app.state::<UpdateState>().set(None);
        if let Ok(file) = pending_file(&app) {
            let _ = std::fs::remove_file(file);
        }
        close_window(&app);
    }
    result
}

async fn run_install(app: &AppHandle) -> Result<(), String> {
    let current = app.package_info().version.to_string();
    if let Some(version) = simulated_version() {
        return simulate_install(app, current, version).await;
    }
    if cfg!(feature = "dev") {
        return Err("the development launcher does not update itself".to_owned());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no update available".to_owned())?;

    let mut pending = PendingUpdate {
        from: current,
        to: update.version.clone(),
        notes: release_notes::list_items(update.body.as_deref().unwrap_or("")),
        phase: UpdatePhase::Download,
    };
    enter(app, &mut pending, UpdatePhase::Download);
    open_window(app)?;

    // The download ends with the signature check, which `download` does before it returns.
    let mut downloaded: u64 = 0;
    let verify_app = app.clone();
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit(PROGRESS_EVENT, Progress { downloaded, total });
            },
            move || {
                let _ = verify_app.emit(PHASE_EVENT, UpdatePhase::Verify);
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    enter(app, &mut pending, UpdatePhase::Install);
    update.install(bytes).map_err(|e| e.to_string())?;
    enter(app, &mut pending, UpdatePhase::Restart);
    restart_into(app, &pending)
}

/// Development only: the update window's steps with a pretend download, ending in a restart that
/// carries on as a real update would.
async fn simulate_install(app: &AppHandle, current: String, version: String) -> Result<(), String> {
    let mut pending = PendingUpdate {
        from: current,
        to: version,
        notes: vec![
            "A simulated update: nothing is downloaded or installed.".to_owned(),
            "Each step takes a moment, as a real update would.".to_owned(),
        ],
        phase: UpdatePhase::Download,
    };
    enter(app, &mut pending, UpdatePhase::Download);
    open_window(app)?;
    let total = 40_000_000u64;
    for step in 1..=40u64 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let _ = app.emit(PROGRESS_EVENT, Progress { downloaded: step * total / 40, total: Some(total) });
    }
    for phase in [UpdatePhase::Verify, UpdatePhase::Install, UpdatePhase::Restart] {
        enter(app, &mut pending, phase);
        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
    }
    restart_into(app, &pending)
}

/// At start: an update that has just been installed shows its window again at once, at the step of
/// preparing the game. One that did not take (the launcher is still the old version) is forgotten.
pub fn resume(app: &AppHandle) {
    let Ok(file) = pending_file(app) else { return };
    let Ok(text) = std::fs::read(&file) else { return };
    let current = app.package_info().version.to_string();
    let pending = serde_json::from_slice::<PendingUpdate>(&text).ok();
    let took = pending.as_ref().is_some_and(|pending| pending.to == current || simulated_version().is_some());
    if !took {
        let _ = std::fs::remove_file(&file);
        return;
    }
    app.state::<UpdateState>().set(pending);
    if let Err(error) = open_window(app) {
        log::warn!("The update window could not open: {error}");
        finish(app);
    }
}

/// As the launcher starts, unless an update carries on (`resume`): the update window opens saying it
/// looks for a newer version, while the main window (hidden at start, tauri.conf.json) loads behind
/// it. The interface looks and then either installs what it found in this same window or ends the
/// check (`end_check`), which shows the launcher.
pub fn open_check(app: &AppHandle) {
    if app.state::<UpdateState>().get().is_some() {
        return;
    }
    let mut pending = PendingUpdate {
        from: app.package_info().version.to_string(),
        to: String::new(),
        notes: Vec::new(),
        phase: UpdatePhase::Check,
    };
    enter(app, &mut pending, UpdatePhase::Check);
    if let Err(error) = open_window(app) {
        log::warn!("The update window could not open for the start's check: {error}");
        finish(app);
        return;
    }
    // Closing the window while it looks must not leave the launcher running with no window: the
    // check ends and the launcher shows.
    if let Some(window) = app.get_webview_window(WINDOW) {
        let closing = app.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                end_check(&closing);
            }
        });
    }
    let watched = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CHECK_GIVE_UP).await;
        if end_check(&watched) {
            log::warn!("The start's update check was never ended: showing the launcher anyway.");
        }
    });
}

/// Ends the start's check when it is still on (nothing was found, or the look failed): its window
/// closes and the launcher shows. An update under way, or one carried across a restart, is left
/// alone. Returns whether the check was on.
pub fn end_check(app: &AppHandle) -> bool {
    let checking = app.state::<UpdateState>().get().is_some_and(|pending| pending.phase == UpdatePhase::Check);
    if checking {
        finish(app);
    }
    checking
}

/// Ends an update: forgets it, closes its window and shows the main one. Returns what it was, if
/// there was one, for the interface to say it is installed.
pub fn finish(app: &AppHandle) -> Option<PendingUpdate> {
    let pending = app.state::<UpdateState>().get();
    app.state::<UpdateState>().set(None);
    if let Ok(file) = pending_file(app) {
        let _ = std::fs::remove_file(file);
    }
    close_window(app);
    pending
}

#[cfg(test)]
mod tests {
    use super::{PendingUpdate, UpdatePhase};

    #[test]
    fn the_check_phase_reaches_the_window_as_check() {
        assert_eq!(serde_json::to_string(&UpdatePhase::Check).unwrap(), "\"check\"");
    }

    #[test]
    fn a_pending_update_survives_the_restart_as_json() {
        let pending = PendingUpdate {
            from: "0.6.13".into(),
            to: "0.7.0".into(),
            notes: vec!["Ranked matches.".into()],
            phase: UpdatePhase::Game,
        };
        let text = serde_json::to_string(&pending).unwrap();
        assert!(text.contains("\"phase\":\"game\""));
        let back: PendingUpdate = serde_json::from_str(&text).unwrap();
        assert_eq!((back.from.as_str(), back.to.as_str(), back.phase), ("0.6.13", "0.7.0", UpdatePhase::Game));
    }
}
