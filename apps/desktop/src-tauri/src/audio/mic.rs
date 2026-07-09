// Microphone capture via cpal. The device's native format is downmixed to
// mono in the audio callback, then resampled to 16kHz s16 on the capture
// thread (cpal streams are !Send, so the whole lifecycle lives on one thread).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, Receiver, Sender};
use tauri::AppHandle;

use super::chunker::{ChunkSink, ChunkSummary, Chunker};
use super::{emit_level, emit_stream_error, StreamKind, LEVEL_EVENT_INTERVAL_MS, SAMPLE_RATE};

pub fn is_available() -> bool {
    cpal::default_host().default_input_device().is_some()
}

pub fn spawn_capture_thread(
    app: AppHandle,
    dir: PathBuf,
    stop_flag: Arc<AtomicBool>,
    epoch: Instant,
    sink: ChunkSink,
) -> Result<JoinHandle<Vec<ChunkSummary>>, String> {
    // Validate the device on the caller thread so start() can report
    // "no microphone" synchronously.
    cpal::default_host()
        .default_input_device()
        .ok_or("no default microphone device")?;

    Ok(std::thread::spawn(move || {
        match capture_loop(&app, &dir, stop_flag, epoch, sink) {
            Ok(chunks) => chunks,
            Err(e) => {
                emit_stream_error(&app, StreamKind::Mic, &e);
                Vec::new()
            }
        }
    }))
}

fn capture_loop(
    app: &AppHandle,
    dir: &std::path::Path,
    stop_flag: Arc<AtomicBool>,
    epoch: Instant,
    sink: ChunkSink,
) -> Result<Vec<ChunkSummary>, String> {
    let mut sink = Some(sink);
    let device = cpal::default_host()
        .default_input_device()
        .ok_or("no default microphone device")?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("mic config: {e}"))?;

    let channels = config.channels() as usize;
    let in_rate = config.sample_rate().0;

    // Audio callbacks must never block: hand mono f32 batches to this thread
    // over a bounded channel (drop-on-full beats blocking the audio thread).
    let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = bounded(64);

    let err_app = app.clone();
    let err_fn = move |e: cpal::StreamError| {
        emit_stream_error(&err_app, StreamKind::Mic, &format!("mic stream: {e}"));
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.clone().into(),
            move |data: &[f32], _| send_mono(&tx, data, channels, |s| s),
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.clone().into(),
            move |data: &[i16], _| send_mono(&tx, data, channels, |s| s as f32 / 32768.0),
            err_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.clone().into(),
            move |data: &[u16], _| {
                send_mono(&tx, data, channels, |s| (s as f32 - 32768.0) / 32768.0)
            },
            err_fn,
            None,
        ),
        other => return Err(format!("unsupported mic sample format: {other:?}")),
    }
    .map_err(|e| format!("build mic stream: {e}"))?;

    stream.play().map_err(|e| format!("start mic stream: {e}"))?;

    let mut resampler = LinearResampler::new(in_rate, SAMPLE_RATE);
    // Created lazily at first data so the stream's real start offset (device
    // open latency) is baked into chunk timestamps.
    let mut chunker: Option<Chunker> = None;
    let mut last_level = Instant::now();

    while !stop_flag.load(Ordering::Relaxed) {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(mono) => {
                let samples = resampler.process(&mono);
                if samples.is_empty() {
                    continue;
                }
                let chunker = chunker.get_or_insert_with(|| {
                    let offset =
                        epoch.elapsed().as_millis() as u64 * SAMPLE_RATE as u64 / 1000;
                    Chunker::new(Some(app.clone()), StreamKind::Mic, dir, offset, sink.take())
                });
                if last_level.elapsed() >= Duration::from_millis(LEVEL_EVENT_INTERVAL_MS) {
                    emit_level(app, StreamKind::Mic, super::rms_i16(&samples));
                    last_level = Instant::now();
                }
                chunker.push(&samples)?;
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }

    drop(stream);
    chunker.map(Chunker::finalize).unwrap_or(Ok(Vec::new()))
}

/// Downmix interleaved samples to mono f32 and send without blocking.
fn send_mono<T: Copy>(
    tx: &Sender<Vec<f32>>,
    data: &[T],
    channels: usize,
    to_f32: impl Fn(T) -> f32,
) {
    if channels == 0 {
        return;
    }
    let mono: Vec<f32> = data
        .chunks_exact(channels)
        .map(|frame| frame.iter().map(|&s| to_f32(s)).sum::<f32>() / channels as f32)
        .collect();
    // try_send: dropping a batch under backpressure is better than stalling
    // the OS audio callback.
    let _ = tx.try_send(mono);
}

/// Stateful linear resampler: good enough for 16kHz speech-to-whisper, keeps
/// fractional position across batches so chunk boundaries stay seamless.
pub struct LinearResampler {
    ratio: f64,
    /// Position in input samples, carried across process() calls.
    pos: f64,
    /// Last sample of the previous batch for interpolation across batches.
    prev: Option<f32>,
    passthrough: bool,
}

impl LinearResampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        Self {
            ratio: in_rate as f64 / out_rate as f64,
            pos: 0.0,
            prev: None,
            passthrough: in_rate == out_rate,
        }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<i16> {
        if input.is_empty() {
            return Vec::new();
        }
        if self.passthrough {
            return input.iter().map(|&s| to_i16(s)).collect();
        }

        // Virtual input: prev sample (if any) prepended at index 0.
        let offset = usize::from(self.prev.is_some());
        let virtual_len = input.len() + offset;
        let at = |i: usize| -> f32 {
            if i < offset {
                self.prev.unwrap()
            } else {
                input[i - offset]
            }
        };

        let mut out = Vec::with_capacity((input.len() as f64 / self.ratio) as usize + 2);
        while self.pos + 1.0 < virtual_len as f64 {
            let i = self.pos as usize;
            let frac = (self.pos - i as f64) as f32;
            let sample = at(i) * (1.0 - frac) + at(i + 1) * frac;
            out.push(to_i16(sample));
            self.pos += self.ratio;
        }

        // Carry state: keep the final sample, rebase position onto it.
        self.pos -= (virtual_len - 1) as f64;
        self.prev = Some(input[input.len() - 1]);
        out
    }
}

fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * 32767.0) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_at_same_rate() {
        let mut r = LinearResampler::new(16_000, 16_000);
        let out = r.process(&[0.0, 0.5, -0.5]);
        assert_eq!(out, vec![0, 16383, -16383]);
    }

    #[test]
    fn halves_sample_count_at_2x_rate() {
        let mut r = LinearResampler::new(32_000, 16_000);
        let input: Vec<f32> = (0..3200).map(|i| (i as f32 / 100.0).sin() * 0.5).collect();
        let out = r.process(&input);
        // 3200 in @32k -> ~1600 out @16k (±1 for boundary carry)
        assert!((out.len() as i64 - 1600).abs() <= 1, "got {}", out.len());
    }

    #[test]
    fn output_count_is_stable_across_batch_sizes() {
        // Same total input split differently must give the same total output ±1.
        let input: Vec<f32> = (0..48_000).map(|i| (i as f32 / 50.0).sin()).collect();

        let mut whole = LinearResampler::new(48_000, 16_000);
        let whole_out = whole.process(&input).len();

        let mut split = LinearResampler::new(48_000, 16_000);
        let mut split_out = 0;
        for batch in input.chunks(441) {
            split_out += split.process(batch).len();
        }
        assert!(
            (whole_out as i64 - split_out as i64).abs() <= 1,
            "whole={whole_out} split={split_out}"
        );
        // 48000 in @48k -> ~16000 out.
        assert!((whole_out as i64 - 16_000).abs() <= 2, "got {whole_out}");
    }
}
