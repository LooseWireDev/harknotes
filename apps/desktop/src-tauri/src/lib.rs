// Harknotes desktop — Tauri v2 native shell.
// Native concerns live here: audio capture, whisper sidecar, storage, tray.

mod audio;
mod db;
mod export;
mod transcription;

use std::sync::{Arc, Mutex};

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};

use audio::{RecordingManager, RecordingStatus, StartedRecording, StoppedRecording};
use db::{Db, MeetingRow, Segment};
use transcription::models::ModelInfo;
use transcription::WorkerHandle;

type ManagedRecording<'a> = State<'a, Mutex<RecordingManager>>;

/// Smoke-test command: verifies the webview → Rust IPC bridge.
#[tauri::command]
fn ping() -> String {
    format!("pong from harknotes-desktop v{}", env!("CARGO_PKG_VERSION"))
}

#[tauri::command]
fn system_audio_available() -> bool {
    audio::system_linux::is_available()
}

#[tauri::command]
fn mic_available() -> bool {
    audio::mic::is_available()
}

#[tauri::command]
fn start_recording(
    app: AppHandle,
    state: ManagedRecording<'_>,
    db: State<'_, Arc<Db>>,
    worker: State<'_, WorkerHandle>,
) -> Result<StartedRecording, String> {
    state
        .lock()
        .map_err(|e| e.to_string())?
        .start(&app, &db, &worker)
}

#[tauri::command]
fn stop_recording(
    app: AppHandle,
    state: ManagedRecording<'_>,
    db: State<'_, Arc<Db>>,
) -> Result<StoppedRecording, String> {
    state.lock().map_err(|e| e.to_string())?.stop(&app, &db)
}

#[tauri::command]
fn recording_status(state: ManagedRecording<'_>) -> Result<RecordingStatus, String> {
    Ok(state.lock().map_err(|e| e.to_string())?.status())
}

#[tauri::command]
fn list_meetings(db: State<'_, Arc<Db>>) -> Result<Vec<MeetingRow>, String> {
    db.list_meetings()
}

#[tauri::command]
fn get_meeting(db: State<'_, Arc<Db>>, meeting_id: String) -> Result<MeetingRow, String> {
    db.get_meeting(&meeting_id)
}

#[tauri::command]
fn rename_meeting(
    db: State<'_, Arc<Db>>,
    meeting_id: String,
    title: String,
) -> Result<(), String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("title cannot be empty".into());
    }
    db.rename_meeting(&meeting_id, trimmed)
}

#[tauri::command]
fn delete_meeting(app: AppHandle, db: State<'_, Arc<Db>>, meeting_id: String) -> Result<(), String> {
    db.delete_meeting(&meeting_id)?;
    // Remove the recording WAVs too; DB row is already gone so a failure here
    // only leaks disk space, never state.
    let dir = audio::recordings_dir(&app)?.join(&meeting_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("delete recordings: {e}"))?;
    }
    Ok(())
}

/// Export a meeting to markdown; returns the written file path.
#[tauri::command]
fn export_meeting(app: AppHandle, db: State<'_, Arc<Db>>, meeting_id: String) -> Result<String, String> {
    export::export_meeting(&app, &db, &meeting_id)
}

#[tauri::command]
fn get_transcript(db: State<'_, Arc<Db>>, meeting_id: String) -> Result<Vec<Segment>, String> {
    db.transcript(&meeting_id)
}

#[tauri::command]
fn list_models(app: AppHandle) -> Result<Vec<ModelInfo>, String> {
    transcription::models::list(&app)
}

/// Fire-and-forget: progress arrives via model:// events.
#[tauri::command]
fn download_model(app: AppHandle, model: String) -> Result<(), String> {
    // Validate before spawning so obvious mistakes fail synchronously.
    transcription::models::model_path(&app, &model)?;
    std::thread::spawn(move || {
        if let Err(e) = transcription::models::download(&app, &model) {
            use tauri::Emitter;
            let _ = app.emit("model://error", serde_json::json!({ "model": model, "error": e }));
        }
    });
    Ok(())
}

#[tauri::command]
fn get_whisper_model(db: State<'_, Arc<Db>>) -> Result<String, String> {
    Ok(db
        .get_setting("whisper_model")?
        .unwrap_or_else(|| transcription::models::DEFAULT_MODEL.to_string()))
}

#[tauri::command]
fn set_whisper_model(db: State<'_, Arc<Db>>, model: String) -> Result<(), String> {
    if !transcription::models::MODELS.iter().any(|&(name, _)| name == model) {
        return Err(format!("unknown whisper model: {model}"));
    }
    db.set_setting("whisper_model", &model)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Mutex::new(RecordingManager::default()))
        .invoke_handler(tauri::generate_handler![
            ping,
            system_audio_available,
            mic_available,
            start_recording,
            stop_recording,
            recording_status,
            list_meetings,
            get_meeting,
            rename_meeting,
            delete_meeting,
            export_meeting,
            get_transcript,
            list_models,
            download_model,
            get_whisper_model,
            set_whisper_model,
        ])
        .setup(|app| {
            // Storage + transcription worker (resumes interrupted chunks).
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db = Arc::new(
                Db::open(&data_dir.join("harknotes.db")).map_err(std::io::Error::other)?,
            );
            app.manage(db.clone());
            app.manage(transcription::spawn(app.handle().clone(), db));

            let show = MenuItem::with_id(app, "show", "Show Harknotes", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
