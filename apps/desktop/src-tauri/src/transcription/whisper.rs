// whisper-cli sidecar invocation: bounded threads, low priority, hard
// timeout, JSON output parsing. One process at a time (the worker is
// single-threaded) — the old app OOM-crashed running two whisper processes
// in parallel.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::db::Segment;

/// Locate the sidecar binary. Tauri places externalBin binaries (without the
/// target-triple suffix) next to the app executable in both dev and bundles.
pub fn sidecar_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let dir = exe.parent().ok_or("executable has no parent dir")?;
    let candidate = dir.join("whisper-cli");
    if candidate.exists() {
        return Ok(candidate);
    }
    Err(format!("whisper-cli sidecar not found at {candidate:?} — run scripts/build-whisper.sh"))
}

/// Threads for whisper: leave headroom for the meeting itself on weak machines.
pub fn whisper_threads() -> usize {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    cores.saturating_sub(2).clamp(1, 4)
}

pub struct WhisperJob<'a> {
    pub wav_path: &'a Path,
    pub model_path: &'a Path,
    pub speaker: &'a str,
    /// Absolute offset of this chunk from recording start.
    pub chunk_start_ms: u64,
    pub chunk_duration_ms: u64,
    /// Tail of the previous chunk's text: biases decoding for continuity.
    pub prompt: Option<&'a str>,
}

pub fn transcribe_chunk(job: &WhisperJob) -> Result<Vec<Segment>, String> {
    let sidecar = sidecar_path()?;
    let out_base = job.wav_path.with_extension(""); // whisper appends .json

    // Hard timeout: 3× real-time, clamped — a hung whisper must never wedge
    // the queue (the old app's core failure mode).
    let timeout_secs = (job.chunk_duration_ms / 1000 * 3).clamp(60, 600);

    let mut cmd = Command::new("nice");
    cmd.arg("-n")
        .arg("10")
        .arg(&sidecar)
        .arg("-m")
        .arg(job.model_path)
        .arg("-f")
        .arg(job.wav_path)
        .arg("-oj") // JSON output file
        .arg("-of")
        .arg(&out_base)
        .arg("-t")
        .arg(whisper_threads().to_string())
        .arg("--no-prints")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(prompt) = job.prompt {
        cmd.arg("--prompt").arg(prompt);
    }

    let mut child = cmd.spawn().map_err(|e| format!("spawn whisper-cli: {e}"))?;

    let started = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| format!("wait whisper-cli: {e}"))? {
            Some(status) => break status,
            None => {
                if started.elapsed() > Duration::from_secs(timeout_secs) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("whisper-cli timed out after {timeout_secs}s"));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    };

    if !status.success() {
        return Err(format!("whisper-cli exited with {status}"));
    }

    let json_path = out_base.with_extension("json");
    let raw = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("read whisper output {json_path:?}: {e}"))?;
    // Output JSON is transient; the segments live in the DB.
    let _ = std::fs::remove_file(&json_path);

    parse_whisper_json(&raw, job.speaker, job.chunk_start_ms)
}

#[derive(Deserialize)]
struct WhisperOutput {
    transcription: Vec<WhisperSegment>,
}

#[derive(Deserialize)]
struct WhisperSegment {
    offsets: WhisperOffsets,
    text: String,
}

#[derive(Deserialize)]
struct WhisperOffsets {
    from: u64,
    to: u64,
}

/// Parse whisper.cpp -oj output into absolute-time segments (unit-testable).
pub fn parse_whisper_json(
    raw: &str,
    speaker: &str,
    chunk_start_ms: u64,
) -> Result<Vec<Segment>, String> {
    let parsed: WhisperOutput =
        serde_json::from_str(raw).map_err(|e| format!("parse whisper json: {e}"))?;
    Ok(parsed
        .transcription
        .into_iter()
        .filter_map(|seg| {
            let text = seg.text.trim().to_string();
            if text.is_empty() {
                return None;
            }
            Some(Segment {
                id: None,
                speaker: speaker.to_string(),
                text,
                start_ms: chunk_start_ms + seg.offsets.from,
                end_ms: chunk_start_ms + seg.offsets.to,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
      "systeminfo": "AVX = 1",
      "model": {"type": "base"},
      "transcription": [
        {"timestamps": {"from": "00:00:00,000", "to": "00:00:02,500"},
         "offsets": {"from": 0, "to": 2500}, "text": " Hello there."},
        {"timestamps": {"from": "00:00:02,500", "to": "00:00:04,000"},
         "offsets": {"from": 2500, "to": 4000}, "text": "   "},
        {"timestamps": {"from": "00:00:04,000", "to": "00:00:06,000"},
         "offsets": {"from": 4000, "to": 6000}, "text": " General Kenobi."}
      ]
    }"#;

    #[test]
    fn parses_and_offsets_segments() {
        let segments = parse_whisper_json(FIXTURE, "User", 45_000).unwrap();
        assert_eq!(segments.len(), 2); // whitespace-only segment dropped
        assert_eq!(segments[0].text, "Hello there.");
        assert_eq!(segments[0].start_ms, 45_000);
        assert_eq!(segments[0].end_ms, 47_500);
        assert_eq!(segments[1].start_ms, 49_000);
        assert_eq!(segments[1].speaker, "User");
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_whisper_json("{not json", "User", 0).is_err());
    }

    #[test]
    fn thread_cap_is_bounded() {
        let t = whisper_threads();
        assert!((1..=4).contains(&t));
    }
}
