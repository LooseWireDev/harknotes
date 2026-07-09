// Whisper model catalog + downloader (Hugging Face, same source the old
// app used). Downloads stream to a .part file and rename on completion so a
// crashed download never leaves a corrupt model behind.

use std::io::{Read, Write};
use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub const DEFAULT_MODEL: &str = "base";

pub const MODELS: &[(&str, u64)] = &[
    // (name, approximate size in MB) — ggml bins from ggerganov/whisper.cpp
    ("tiny", 78),
    ("base", 148),
    ("small", 488),
    ("medium", 1533),
    ("large-v3-turbo", 1620),
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub name: String,
    pub size_mb: u64,
    pub downloaded: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProgress {
    pub model: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

pub fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?
        .join("models");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create models dir: {e}"))?;
    Ok(dir)
}

pub fn model_path(app: &AppHandle, model: &str) -> Result<PathBuf, String> {
    validate(model)?;
    Ok(models_dir(app)?.join(format!("ggml-{model}.bin")))
}

pub fn list(app: &AppHandle) -> Result<Vec<ModelInfo>, String> {
    let dir = models_dir(app)?;
    Ok(MODELS
        .iter()
        .map(|&(name, size_mb)| ModelInfo {
            name: name.to_string(),
            size_mb,
            downloaded: dir.join(format!("ggml-{name}.bin")).exists(),
        })
        .collect())
}

fn validate(model: &str) -> Result<(), String> {
    if MODELS.iter().any(|&(name, _)| name == model) {
        Ok(())
    } else {
        Err(format!("unknown whisper model: {model}"))
    }
}

/// Blocking download with progress events; call from a background thread.
pub fn download(app: &AppHandle, model: &str) -> Result<PathBuf, String> {
    let dest = model_path(app, model)?;
    if dest.exists() {
        return Ok(dest);
    }

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{model}.bin"
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(None) // large files on slow links; progress below tracks liveness
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("download {model}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download {model}: {e}"))?;

    let total_bytes = resp.content_length().unwrap_or(0);
    let part = dest.with_extension("bin.part");
    let mut file = std::fs::File::create(&part).map_err(|e| format!("create {part:?}: {e}"))?;

    let mut downloaded: u64 = 0;
    let mut last_emitted: u64 = 0;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut buf).map_err(|e| format!("download {model}: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| format!("write model: {e}"))?;
        downloaded += n as u64;
        // Emit roughly every 1% (or every 8MB when size is unknown).
        let step = if total_bytes > 0 { total_bytes / 100 } else { 8 * 1024 * 1024 };
        if downloaded - last_emitted >= step.max(1) {
            last_emitted = downloaded;
            let _ = app.emit(
                "model://progress",
                ModelProgress {
                    model: model.to_string(),
                    downloaded_bytes: downloaded,
                    total_bytes,
                },
            );
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    std::fs::rename(&part, &dest).map_err(|e| format!("finalize model file: {e}"))?;
    let _ = app.emit(
        "model://progress",
        ModelProgress {
            model: model.to_string(),
            downloaded_bytes: downloaded,
            total_bytes: downloaded.max(total_bytes),
        },
    );
    Ok(dest)
}
