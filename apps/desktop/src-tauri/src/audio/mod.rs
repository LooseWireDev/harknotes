// Audio capture: dual-stream (mic via cpal, system via parec), 16kHz mono s16,
// cut into ~45-60s chunks at silence boundaries for incremental transcription.

pub mod chunker;
pub mod mic;
pub mod recovery;
pub mod system_linux;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::db::{ChunkStatus, Db};
use crate::transcription::{ChunkJob, WorkerHandle};
use chunker::{ChunkSink, ChunkSummary};

/// Whisper.cpp input format: 16kHz, mono, signed 16-bit PCM.
pub const SAMPLE_RATE: u32 = 16_000;

/// Throttle for level events sent to the waveform UI.
pub const LEVEL_EVENT_INTERVAL_MS: u64 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    Mic,
    System,
}

impl StreamKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StreamKind::Mic => "mic",
            StreamKind::System => "system",
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelEvent {
    pub stream: StreamKind,
    /// RMS normalized to 0.0..=1.0.
    pub rms: f32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DurationEvent {
    pub seconds: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamErrorEvent {
    pub stream: StreamKind,
    pub message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedRecording {
    pub meeting_id: String,
    pub recordings_dir: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoppedRecording {
    pub meeting_id: String,
    pub duration_seconds: u64,
    pub mic_chunks: Vec<ChunkSummary>,
    pub system_chunks: Vec<ChunkSummary>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub recording: bool,
    pub meeting_id: Option<String>,
    pub duration_seconds: u64,
}

struct ActiveRecording {
    meeting_id: String,
    started_at: Instant,
    stop_flag: Arc<AtomicBool>,
    mic_handle: Option<JoinHandle<Vec<ChunkSummary>>>,
    system_handle: Option<JoinHandle<Vec<ChunkSummary>>>,
    ticker_handle: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct RecordingManager {
    active: Option<ActiveRecording>,
}

/// Persist the chunk row and hand it to the transcription queue, synchronously
/// at cut time — a chunk on disk is always either in the DB or not yet counted.
fn make_sink(db: Arc<Db>, worker: WorkerHandle, meeting_id: String) -> ChunkSink {
    Box::new(move |c: &ChunkSummary| {
        let status = if c.silent { ChunkStatus::Silent } else { ChunkStatus::Pending };
        if let Err(e) = db.insert_chunk(
            &meeting_id,
            c.stream.as_str(),
            c.index,
            &c.path,
            c.start_ms,
            c.duration_ms,
            status,
        ) {
            eprintln!("[audio] persist chunk row: {e}");
        }
        worker.enqueue(ChunkJob {
            meeting_id: meeting_id.clone(),
            stream: c.stream.as_str().to_string(),
            idx: c.index,
            wav_path: c.path.clone(),
            start_ms: c.start_ms,
            duration_ms: c.duration_ms,
            silent: c.silent,
            already_persisted: true,
        });
    })
}

impl RecordingManager {
    pub fn start(
        &mut self,
        app: &AppHandle,
        db: &Arc<Db>,
        worker: &WorkerHandle,
    ) -> Result<StartedRecording, String> {
        if self.active.is_some() {
            return Err("already recording".into());
        }

        let now = chrono::Local::now();
        let meeting_id = now.format("%Y-%m-%d_%H-%M-%S").to_string();
        let dir = recordings_dir(app)?.join(&meeting_id);
        std::fs::create_dir_all(&dir).map_err(|e| format!("create recording dir: {e}"))?;

        let model = db
            .get_setting("whisper_model")?
            .unwrap_or_else(|| crate::transcription::models::DEFAULT_MODEL.to_string());
        let title = now.format("Meeting %b %-d, %-H:%M").to_string();
        db.insert_meeting(&meeting_id, &title, &model)?;

        let stop_flag = Arc::new(AtomicBool::new(false));
        let epoch = Instant::now();

        let system_handle = system_linux::spawn_capture_thread(
            app.clone(),
            dir.clone(),
            stop_flag.clone(),
            epoch,
            make_sink(db.clone(), worker.clone(), meeting_id.clone()),
        )?;
        // Mic failure should not abort the whole recording (system audio alone is
        // still a useful transcript) — surface it as a stream error event instead.
        let mic_handle = match mic::spawn_capture_thread(
            app.clone(),
            dir.clone(),
            stop_flag.clone(),
            epoch,
            make_sink(db.clone(), worker.clone(), meeting_id.clone()),
        ) {
            Ok(handle) => Some(handle),
            Err(message) => {
                emit_stream_error(app, StreamKind::Mic, &message);
                None
            }
        };

        let ticker_handle = spawn_duration_ticker(app.clone(), stop_flag.clone());

        self.active = Some(ActiveRecording {
            meeting_id: meeting_id.clone(),
            started_at: Instant::now(),
            stop_flag,
            mic_handle,
            system_handle: Some(system_handle),
            ticker_handle: Some(ticker_handle),
        });

        Ok(StartedRecording {
            meeting_id,
            recordings_dir: dir.to_string_lossy().into_owned(),
        })
    }

    pub fn stop(&mut self, app: &AppHandle, db: &Arc<Db>) -> Result<StoppedRecording, String> {
        let mut active = self.active.take().ok_or("not recording")?;
        active.stop_flag.store(true, Ordering::Relaxed);

        let mic_chunks = active
            .mic_handle
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        let system_chunks = active
            .system_handle
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        if let Some(ticker) = active.ticker_handle.take() {
            let _ = ticker.join();
        }

        let duration_seconds = active.started_at.elapsed().as_secs();
        db.finish_recording(&active.meeting_id, duration_seconds)?;
        // Everything may already be transcribed (chunks are processed live).
        if db.try_mark_ready(&active.meeting_id)? {
            let _ = app.emit(
                "transcribe://meeting-ready",
                crate::transcription::MeetingReadyEvent {
                    meeting_id: active.meeting_id.clone(),
                },
            );
        }

        Ok(StoppedRecording {
            meeting_id: active.meeting_id,
            duration_seconds,
            mic_chunks,
            system_chunks,
        })
    }

    pub fn status(&self) -> RecordingStatus {
        match &self.active {
            Some(active) => RecordingStatus {
                recording: true,
                meeting_id: Some(active.meeting_id.clone()),
                duration_seconds: active.started_at.elapsed().as_secs(),
            },
            None => RecordingStatus {
                recording: false,
                meeting_id: None,
                duration_seconds: 0,
            },
        }
    }
}

pub fn recordings_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    Ok(base.join("recordings"))
}

pub fn emit_level(app: &AppHandle, stream: StreamKind, rms: f32) {
    let _ = app.emit("recording://level", LevelEvent { stream, rms });
}

pub fn emit_stream_error(app: &AppHandle, stream: StreamKind, message: &str) {
    eprintln!("[audio:{}] {message}", stream.as_str());
    let _ = app.emit(
        "recording://error",
        StreamErrorEvent {
            stream,
            message: message.to_string(),
        },
    );
}

/// Normalized RMS (0.0..=1.0) of a batch of s16 samples.
pub fn rms_i16(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|&s| {
            let f = s as f64 / 32768.0;
            f * f
        })
        .sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

fn spawn_duration_ticker(app: AppHandle, stop_flag: Arc<AtomicBool>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let started = Instant::now();
        while !stop_flag.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            let _ = app.emit(
                "recording://duration",
                DurationEvent {
                    seconds: started.elapsed().as_secs(),
                },
            );
        }
    })
}
