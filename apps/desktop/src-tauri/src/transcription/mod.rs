// Transcription worker: a single background thread draining a job queue.
// Exactly one whisper process runs at any time (two parallel runs OOM-killed
// the old app). Chunks are persisted before transcription, so a crash or
// quit mid-queue resumes on next launch via Db::resumable_chunks().

pub mod models;
pub mod whisper;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::db::{ChunkStatus, Db, Segment};

/// Tail length of previous-chunk text used as a continuity prompt.
const PROMPT_TAIL_CHARS: usize = 200;

#[derive(Clone)]
pub struct ChunkJob {
    pub meeting_id: String,
    pub stream: String, // "mic" | "system"
    pub idx: u32,
    pub wav_path: String,
    pub start_ms: u64,
    pub duration_ms: u64,
    pub silent: bool,
    /// Already persisted (resume path) vs fresh from the chunker.
    pub already_persisted: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkDoneEvent {
    pub meeting_id: String,
    pub stream: String,
    pub idx: u32,
    pub segments: Vec<Segment>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkFailedEvent {
    pub meeting_id: String,
    pub stream: String,
    pub idx: u32,
    pub error: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingReadyEvent {
    pub meeting_id: String,
}

#[derive(Clone)]
pub struct WorkerHandle {
    tx: Sender<ChunkJob>,
}

impl WorkerHandle {
    pub fn enqueue(&self, job: ChunkJob) {
        let _ = self.tx.send(job);
    }
}

pub fn spawn(app: AppHandle, db: Arc<Db>) -> WorkerHandle {
    let (tx, rx) = unbounded::<ChunkJob>();

    // Resume anything interrupted by a previous crash/quit.
    match db.resumable_chunks() {
        Ok(chunks) => {
            for c in chunks {
                let _ = tx.send(ChunkJob {
                    meeting_id: c.meeting_id,
                    stream: c.stream,
                    idx: c.idx,
                    wav_path: c.wav_path,
                    start_ms: c.start_ms,
                    duration_ms: c.duration_ms,
                    silent: false,
                    already_persisted: true,
                });
            }
        }
        Err(e) => eprintln!("[transcription] resume scan failed: {e}"),
    }

    std::thread::spawn({
        let handle_rx: Receiver<ChunkJob> = rx;
        move || worker_loop(app, db, handle_rx)
    });

    WorkerHandle { tx }
}

fn worker_loop(app: AppHandle, db: Arc<Db>, rx: Receiver<ChunkJob>) {
    // (meeting, stream) -> tail of last transcribed text, for --prompt.
    let mut prompt_tails: HashMap<(String, String), String> = HashMap::new();

    while let Ok(job) = rx.recv() {
        if !job.already_persisted {
            let status = if job.silent { ChunkStatus::Silent } else { ChunkStatus::Pending };
            if let Err(e) = db.insert_chunk(
                &job.meeting_id,
                &job.stream,
                job.idx,
                &job.wav_path,
                job.start_ms,
                job.duration_ms,
                status,
            ) {
                eprintln!("[transcription] persist chunk: {e}");
            }
        }

        if job.silent {
            check_ready(&app, &db, &job.meeting_id);
            continue;
        }

        let model = db
            .get_setting("whisper_model")
            .ok()
            .flatten()
            .unwrap_or_else(|| models::DEFAULT_MODEL.to_string());
        let result = run_job(&app, &job, &model, &mut prompt_tails);

        match result {
            Ok(segments) => {
                if let Err(e) = db.complete_chunk(&job.meeting_id, &job.stream, job.idx, &segments)
                {
                    eprintln!("[transcription] save segments: {e}");
                }
                let _ = app.emit(
                    "transcribe://chunk-done",
                    ChunkDoneEvent {
                        meeting_id: job.meeting_id.clone(),
                        stream: job.stream.clone(),
                        idx: job.idx,
                        segments,
                    },
                );
            }
            Err(error) => {
                eprintln!(
                    "[transcription] chunk {}/{}/{} failed: {error}",
                    job.meeting_id, job.stream, job.idx
                );
                if let Err(e) = db.fail_chunk(&job.meeting_id, &job.stream, job.idx, &error) {
                    eprintln!("[transcription] mark failed: {e}");
                }
                let _ = app.emit(
                    "transcribe://chunk-failed",
                    ChunkFailedEvent {
                        meeting_id: job.meeting_id.clone(),
                        stream: job.stream.clone(),
                        idx: job.idx,
                        error,
                    },
                );
            }
        }

        check_ready(&app, &db, &job.meeting_id);
    }
}

fn run_job(
    app: &AppHandle,
    job: &ChunkJob,
    model: &str,
    prompt_tails: &mut HashMap<(String, String), String>,
) -> Result<Vec<Segment>, String> {
    let model_path = models::model_path(app, model)?;
    if !model_path.exists() {
        // Auto-download on first use (blocking inside the worker keeps the
        // one-job-at-a-time invariant).
        models::download(app, model)?;
    }

    let speaker = if job.stream == "mic" { "User" } else { "Meeting" };
    let key = (job.meeting_id.clone(), job.stream.clone());
    let prompt = prompt_tails.get(&key).cloned();

    // One retry: transient failures (OOM kill, fs hiccup) shouldn't lose a chunk.
    let mut last_err = String::new();
    for attempt in 0..2 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        match whisper::transcribe_chunk(&whisper::WhisperJob {
            wav_path: Path::new(&job.wav_path),
            model_path: &model_path,
            speaker,
            chunk_start_ms: job.start_ms,
            chunk_duration_ms: job.duration_ms,
            prompt: prompt.as_deref(),
        }) {
            Ok(segments) => {
                let joined: String = segments
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let tail: String = joined
                    .chars()
                    .rev()
                    .take(PROMPT_TAIL_CHARS)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                if !tail.is_empty() {
                    prompt_tails.insert(key, tail);
                }
                return Ok(segments);
            }
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

fn check_ready(app: &AppHandle, db: &Db, meeting_id: &str) {
    match db.try_mark_ready(meeting_id) {
        Ok(true) => {
            let _ = app.emit(
                "transcribe://meeting-ready",
                MeetingReadyEvent { meeting_id: meeting_id.to_string() },
            );
        }
        Ok(false) => {}
        Err(e) => eprintln!("[transcription] readiness check: {e}"),
    }
}
