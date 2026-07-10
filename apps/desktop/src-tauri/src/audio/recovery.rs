// Startup recovery: promote crash-leftover buffer snapshots
// (recovery-<stream>.{wav,json}, written every ~5s by the chunker) into real
// chunk rows so their audio gets transcribed instead of lost.

use std::path::Path;
use std::sync::Arc;

use crate::db::{ChunkStatus, Db};

use super::chunker::{recovery_meta_path, recovery_wav_path, RecoveryMeta};
use super::{rms_i16, StreamKind, SAMPLE_RATE};

/// Scan all meeting recording dirs and recover pending snapshots into chunk
/// rows. Call before the resume scan (which enqueues all pending chunks) and
/// before stale-recording finalization (so recovered chunks count toward
/// duration).
pub fn recover_snapshots(recordings_root: &Path, db: &Arc<Db>) {
    let Ok(entries) = std::fs::read_dir(recordings_root) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let meeting_id = entry.file_name().to_string_lossy().into_owned();
        // Orphan dirs (no DB row, e.g. pre-DB tests) are not ours to touch.
        if db.get_meeting(&meeting_id).is_err() {
            continue;
        }
        for stream in [StreamKind::Mic, StreamKind::System] {
            if let Err(e) = recover_stream(&dir, &meeting_id, stream, db) {
                eprintln!("[recovery] {meeting_id}/{}: {e}", stream.as_str());
            }
        }
    }
}

fn recover_stream(
    dir: &Path,
    meeting_id: &str,
    stream: StreamKind,
    db: &Arc<Db>,
) -> Result<(), String> {
    let wav_path = recovery_wav_path(dir, stream);
    let meta_path = recovery_meta_path(dir, stream);
    if !wav_path.exists() || !meta_path.exists() {
        return Ok(());
    }

    let meta: RecoveryMeta = serde_json::from_str(
        &std::fs::read_to_string(&meta_path).map_err(|e| format!("read meta: {e}"))?,
    )
    .map_err(|e| format!("parse meta: {e}"))?;

    // A snapshot truncated mid-write parses as an invalid WAV — skip quietly.
    let samples = read_wav_samples(&wav_path)?;
    if samples.is_empty() {
        cleanup(&wav_path, &meta_path);
        return Ok(());
    }

    let silent = rms_i16(&samples) < super::chunker::SILENT_CHUNK_RMS;
    let duration_ms = samples.len() as u64 * 1000 / SAMPLE_RATE as u64;

    // Promote the snapshot to a real chunk file.
    let chunk_path = dir.join(format!("chunk-{:04}-{}.wav", meta.index, stream.as_str()));
    std::fs::rename(&wav_path, &chunk_path).map_err(|e| format!("promote snapshot: {e}"))?;
    let _ = std::fs::remove_file(&meta_path);

    let status = if silent { ChunkStatus::Silent } else { ChunkStatus::Pending };
    db.insert_chunk(
        meeting_id,
        stream.as_str(),
        meta.index,
        &chunk_path.to_string_lossy(),
        meta.start_ms,
        duration_ms,
        status,
    )?;
    eprintln!(
        "[recovery] restored {}s of {} audio for {meeting_id}",
        duration_ms / 1000,
        stream.as_str()
    );
    Ok(())
}

fn read_wav_samples(path: &Path) -> Result<Vec<i16>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("open wav: {e}"))?;
    reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("read wav: {e}"))
}

fn cleanup(wav: &Path, meta: &Path) {
    let _ = std::fs::remove_file(wav);
    let _ = std::fs::remove_file(meta);
}
