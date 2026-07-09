// System-audio capture on Linux via `parec` reading the default sink's
// monitor source. parec (PulseAudio compat layer) is used deliberately
// instead of pw-record, which can silently fall back to the microphone
// when routing monitor nodes. Port of the proven Electron adapter.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tauri::AppHandle;

use super::chunker::{ChunkSummary, Chunker};
use super::{emit_level, emit_stream_error, StreamKind, LEVEL_EVENT_INTERVAL_MS};

pub fn is_available() -> bool {
    let pactl_ok = Command::new("pactl")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let parec_ok = Command::new("parec")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    pactl_ok && parec_ok
}

/// Find the monitor source of the default output sink.
pub fn get_monitor_source() -> Result<String, String> {
    let default_sink = run_pactl(&["get-default-sink"])
        .map(|out| out.trim().to_string())
        .unwrap_or_default();

    // Parse `pactl list sources` for "Monitor of Sink:" associations rather
    // than guessing names, so alternate PipeWire naming still works.
    if let Ok(out) = run_pactl(&["list", "sources"]) {
        if let Some(name) = parse_monitor_source(&out, &default_sink) {
            return Ok(name);
        }
    }

    // Fallback: the common PipeWire convention.
    if !default_sink.is_empty() {
        return Ok(format!("{default_sink}.monitor"));
    }

    // Last resort: first source ending in .monitor.
    if let Ok(out) = run_pactl(&["list", "short", "sources"]) {
        for line in out.lines() {
            if let Some(name) = line.split_whitespace().nth(1) {
                if name.ends_with(".monitor") {
                    return Ok(name.to_string());
                }
            }
        }
    }

    Err("could not find a PulseAudio/PipeWire monitor source".into())
}

/// Pure parser for `pactl list sources` output (unit-testable).
fn parse_monitor_source(pactl_output: &str, default_sink: &str) -> Option<String> {
    let mut current_name = String::new();
    for line in pactl_output.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Name: ") {
            current_name = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Monitor of Sink: ") {
            let sink = rest.trim();
            if !current_name.is_empty() && (default_sink.is_empty() || sink == default_sink) {
                return Some(current_name);
            }
        }
    }
    None
}

fn run_pactl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|e| format!("pactl {args:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "pactl {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn spawn_capture_thread(
    app: AppHandle,
    dir: std::path::PathBuf,
    stop_flag: Arc<AtomicBool>,
    epoch: Instant,
) -> Result<JoinHandle<Vec<ChunkSummary>>, String> {
    let monitor = get_monitor_source()?;
    let child = spawn_parec(&monitor)?;

    Ok(std::thread::spawn(move || {
        capture_loop(app, &dir, child, stop_flag, epoch)
    }))
}

fn spawn_parec(monitor: &str) -> Result<Child, String> {
    // --latency-msec=20 delivers ~20ms batches for smooth level updates.
    Command::new("parec")
        .args([
            "--device",
            monitor,
            "--rate",
            "16000",
            "--channels",
            "1",
            "--format",
            "s16le",
            "--raw",
            "--latency-msec",
            "20",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn parec: {e}"))
}

fn capture_loop(
    app: AppHandle,
    dir: &Path,
    mut child: Child,
    stop_flag: Arc<AtomicBool>,
    epoch: Instant,
) -> Vec<ChunkSummary> {
    let mut stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            emit_stream_error(&app, StreamKind::System, "parec produced no stdout");
            return Vec::new();
        }
    };

    // Created lazily at first data so the stream's real start offset (device
    // open latency) is baked into chunk timestamps.
    let mut chunker: Option<Chunker> = None;
    // 200ms of s16 mono @16kHz per read.
    let mut buf = vec![0u8; 6400];
    let mut pending_byte: Option<u8> = None;
    let mut last_level = Instant::now();
    let mut received_any = false;

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
        match stdout.read(&mut buf) {
            Ok(0) => {
                if !stop_flag.load(Ordering::Relaxed) {
                    emit_stream_error(
                        &app,
                        StreamKind::System,
                        if received_any {
                            "parec stream ended unexpectedly"
                        } else {
                            "parec exited before producing data — monitor source may not exist"
                        },
                    );
                }
                break;
            }
            Ok(n) => {
                received_any = true;
                let samples = bytes_to_i16(&mut pending_byte, &buf[..n]);
                if samples.is_empty() {
                    continue;
                }
                let chunker = chunker.get_or_insert_with(|| {
                    let offset =
                        epoch.elapsed().as_millis() as u64 * super::SAMPLE_RATE as u64 / 1000;
                    Chunker::new(Some(app.clone()), StreamKind::System, dir, offset)
                });
                if last_level.elapsed() >= Duration::from_millis(LEVEL_EVENT_INTERVAL_MS) {
                    emit_level(&app, StreamKind::System, super::rms_i16(&samples));
                    last_level = Instant::now();
                }
                if let Err(e) = chunker.push(&samples) {
                    emit_stream_error(&app, StreamKind::System, &e);
                    break;
                }
            }
            Err(e) => {
                if !stop_flag.load(Ordering::Relaxed) {
                    emit_stream_error(&app, StreamKind::System, &format!("parec read: {e}"));
                }
                break;
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    match chunker.map(Chunker::finalize).unwrap_or(Ok(Vec::new())) {
        Ok(chunks) => chunks,
        Err(e) => {
            emit_stream_error(&app, StreamKind::System, &e);
            Vec::new()
        }
    }
}

/// Convert little-endian bytes to i16 samples, carrying a dangling odd byte
/// across reads.
fn bytes_to_i16(pending: &mut Option<u8>, bytes: &[u8]) -> Vec<i16> {
    let mut data: Vec<u8>;
    let slice: &[u8] = match pending.take() {
        Some(b) => {
            data = Vec::with_capacity(bytes.len() + 1);
            data.push(b);
            data.extend_from_slice(bytes);
            &data
        }
        None => bytes,
    };
    let full_pairs = slice.len() / 2;
    if slice.len() % 2 == 1 {
        *pending = Some(slice[slice.len() - 1]);
    }
    (0..full_pairs)
        .map(|i| i16::from_le_bytes([slice[2 * i], slice[2 * i + 1]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACTL_FIXTURE: &str = r#"Source #55
	State: SUSPENDED
	Name: alsa_output.pci-0000_00_1f.3.analog-stereo.monitor
	Description: Monitor of Built-in Audio Analog Stereo
	Monitor of Sink: alsa_output.pci-0000_00_1f.3.analog-stereo
	Volume: front-left: 65536 / 100%
Source #56
	State: RUNNING
	Name: alsa_input.pci-0000_00_1f.3.analog-stereo
	Description: Built-in Audio Analog Stereo
	Monitor of Sink: n/a
"#;

    #[test]
    fn parses_monitor_of_default_sink() {
        let name = parse_monitor_source(
            PACTL_FIXTURE,
            "alsa_output.pci-0000_00_1f.3.analog-stereo",
        );
        assert_eq!(
            name.as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo.monitor")
        );
    }

    #[test]
    fn accepts_any_monitor_without_default_sink() {
        let name = parse_monitor_source(PACTL_FIXTURE, "");
        assert_eq!(
            name.as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo.monitor")
        );
    }

    #[test]
    fn returns_none_when_no_monitor_matches() {
        assert_eq!(parse_monitor_source(PACTL_FIXTURE, "some-other-sink"), None);
    }

    #[test]
    fn converts_bytes_with_dangling_carry() {
        let mut pending = None;
        // 0x0001 LE, plus dangling 0xFF
        let first = bytes_to_i16(&mut pending, &[0x01, 0x00, 0xFF]);
        assert_eq!(first, vec![1]);
        assert_eq!(pending, Some(0xFF));
        // 0xFF | 0x7F00 -> 0x7FFF LE
        let second = bytes_to_i16(&mut pending, &[0x7F]);
        assert_eq!(second, vec![i16::MAX]);
        assert_eq!(pending, None);
    }
}
