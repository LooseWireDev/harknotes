// Harknotes desktop — Tauri v2 native shell.
// Native concerns live here: audio capture, whisper sidecar, storage, tray.

mod audio;

use std::sync::Mutex;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};

use audio::{RecordingManager, RecordingStatus, StartedRecording, StoppedRecording};

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
) -> Result<StartedRecording, String> {
    state.lock().map_err(|e| e.to_string())?.start(&app)
}

#[tauri::command]
fn stop_recording(state: ManagedRecording<'_>) -> Result<StoppedRecording, String> {
    state.lock().map_err(|e| e.to_string())?.stop()
}

#[tauri::command]
fn recording_status(state: ManagedRecording<'_>) -> Result<RecordingStatus, String> {
    Ok(state.lock().map_err(|e| e.to_string())?.status())
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
        ])
        .setup(|app| {
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
