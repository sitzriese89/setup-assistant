// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;
use setup_core::live::{LiveSession, LiveStatus};
use setup_core::{BehaviorInfo, CarContext, History, ParamInfo, RuleBook, Suggestion, Symptom};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

struct AppState {
    rules: RuleBook,
    live: LiveSession,
    recordings: Option<PathBuf>,
}

#[derive(Serialize)]
struct Catalog {
    behaviors: Vec<BehaviorInfo>,
    params: BTreeMap<String, ParamInfo>,
}

/// Everything the UI needs to build the symptom picker.
#[tauri::command]
fn catalog(state: tauri::State<AppState>) -> Catalog {
    Catalog { behaviors: state.rules.behaviors.clone(), params: state.rules.params.clone() }
}

#[tauri::command]
fn symptom_label(state: tauri::State<AppState>, symptom: Symptom) -> String {
    state.rules.symptom_label(&symptom)
}

/// Suggestions for the current problems, taking earlier changes and fixes into account.
#[tauri::command]
fn suggest(
    state: tauri::State<AppState>,
    car: CarContext,
    symptoms: Vec<Symptom>,
    history: Option<History>,
) -> Vec<Suggestion> {
    state.rules.suggest_with_history(&car, &symptoms, &history.unwrap_or_default())
}

/// Connection state, live values and detected problems. The UI polls this.
#[tauri::command]
fn live_status(state: tauri::State<AppState>) -> LiveStatus {
    state.live.status(&state.rules)
}

#[tauri::command]
fn live_reset(state: tauri::State<AppState>) {
    state.live.reset();
}

/// Starts run `run` (the UI keeps the count; it survives app restarts).
#[tauri::command]
fn live_next_run(state: tauri::State<AppState>, run: Option<u32>) {
    match run {
        Some(r) => state.live.start_run(r),
        None => state.live.next_run(),
    }
}

#[tauri::command]
fn live_set_run(state: tauri::State<AppState>, run: u32) {
    state.live.set_run(run);
}

#[tauri::command]
fn live_new_session(state: tauri::State<AppState>) {
    state.live.new_session();
}

#[tauri::command]
fn live_set_demo(state: tauri::State<AppState>, on: bool) {
    state.live.set_demo(on);
}

/// Gives a corner on the current track map a name (empty clears it).
#[tauri::command]
fn track_rename(state: tauri::State<AppState>, corner: u32, name: String) {
    state.live.rename_corner(corner, name);
}

#[tauri::command]
fn recordings_dir(state: tauri::State<AppState>) -> Option<String> {
    state.recordings.as_ref().map(|p| p.display().to_string())
}

/// Small always-on-top window for use while driving. Async: creating windows
/// from a sync command can deadlock on Windows.
#[tauri::command]
async fn overlay_show(app: AppHandle, show: bool) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        if !show {
            w.close().map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    if show {
        WebviewWindowBuilder::new(&app, "overlay", WebviewUrl::App("overlay.html".into()))
            .title("Setup Assistant overlay")
            .inner_size(420.0, 132.0)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .build()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Locked = clicks go through to the game underneath.
#[tauri::command]
fn overlay_lock(app: AppHandle, lock: bool) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        w.set_ignore_cursor_events(lock).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() {
    let rules = RuleBook::builtin();
    let problems = rules.validate();
    if !problems.is_empty() {
        eprintln!("rule book problems:\n  {}", problems.join("\n  "));
    }

    tauri::Builder::default()
        .setup(move |app| {
            let data = app.path().app_data_dir().ok();
            let recordings = data.as_ref().map(|d| d.join("recordings"));
            let tracks = data.as_ref().map(|d| d.join("tracks"));
            let live =
                LiveSession::start_with_tracks(Box::new(r3e_telemetry::R3eSource::new()), recordings.clone(), tracks);
            app.manage(AppState { rules, live, recordings });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            catalog,
            symptom_label,
            suggest,
            live_status,
            live_reset,
            live_next_run,
            live_set_run,
            live_new_session,
            live_set_demo,
            recordings_dir,
            track_rename,
            overlay_show,
            overlay_lock
        ])
        .run(tauri::generate_context!())
        .expect("error while running the app");
}
