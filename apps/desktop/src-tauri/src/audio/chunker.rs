// Cuts a continuous 16kHz mono s16 stream into WAV chunks for incremental
// transcription. Chunks target ~45s and are cut at silence boundaries so
// whisper never splits mid-word; hard cap at 60s cutting at the quietest
// 200ms frame seen past the target.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::{StreamKind, SAMPLE_RATE};

/// Preferred chunk length before we start looking for a silence boundary.
pub const TARGET_SAMPLES: usize = 45 * SAMPLE_RATE as usize;
/// Hard cap: never let a chunk exceed this.
pub const MAX_SAMPLES: usize = 60 * SAMPLE_RATE as usize;
/// Boundary-scan frame: 200ms.
pub const FRAME_SAMPLES: usize = SAMPLE_RATE as usize / 5;
/// A frame quieter than this (normalized RMS) is a good cut point.
pub const SILENCE_FRAME_RMS: f32 = 0.008;
/// A whole chunk quieter than this is marked silent and skipped by
/// transcription — this is the gate that kills whisper's "thank you"
/// hallucinations on dead air.
pub const SILENT_CHUNK_RMS: f32 = 0.004;
/// Don't bother writing a trailing chunk shorter than this on finalize.
const MIN_FINAL_SAMPLES: usize = SAMPLE_RATE as usize / 2;

/// Called synchronously after each chunk WAV is written — used to persist the
/// chunk row and enqueue transcription before anything else can go wrong.
pub type ChunkSink = Box<dyn FnMut(&ChunkSummary) + Send>;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkSummary {
    pub stream: StreamKind,
    pub index: u32,
    pub path: String,
    /// Offset of the chunk start from the beginning of the recording.
    pub start_ms: u64,
    pub duration_ms: u64,
    /// True when the whole chunk is below the silence floor — transcription skips it.
    pub silent: bool,
}

pub struct Chunker {
    app: Option<AppHandle>,
    stream: StreamKind,
    dir: PathBuf,
    buffer: Vec<i16>,
    chunk_index: u32,
    /// Absolute sample offset (since recording start) of buffer[0].
    buffer_start_sample: u64,
    /// Quietest 200ms frame found past TARGET_SAMPLES: (end_index, rms).
    quietest_frame: Option<(usize, f32)>,
    /// Next frame boundary to scan from.
    scan_pos: usize,
    written: Vec<ChunkSummary>,
    sink: Option<ChunkSink>,
}

impl Chunker {
    /// `start_offset_samples`: how far into the recording this stream's first
    /// sample actually is (device open latency differs per stream), so chunk
    /// timestamps stay aligned across streams at merge time.
    pub fn new(
        app: Option<AppHandle>,
        stream: StreamKind,
        dir: &Path,
        start_offset_samples: u64,
        sink: Option<ChunkSink>,
    ) -> Self {
        Self {
            app,
            stream,
            dir: dir.to_path_buf(),
            buffer: Vec::with_capacity(MAX_SAMPLES),
            chunk_index: 0,
            buffer_start_sample: start_offset_samples,
            quietest_frame: None,
            scan_pos: TARGET_SAMPLES,
            written: Vec::new(),
            sink,
        }
    }

    pub fn push(&mut self, samples: &[i16]) -> Result<(), String> {
        self.buffer.extend_from_slice(samples);

        // Scan complete 200ms frames in the boundary region [TARGET..].
        while self.scan_pos + FRAME_SAMPLES <= self.buffer.len() {
            let frame = &self.buffer[self.scan_pos..self.scan_pos + FRAME_SAMPLES];
            let rms = super::rms_i16(frame);
            let end = self.scan_pos + FRAME_SAMPLES;

            if rms < SILENCE_FRAME_RMS {
                // Real silence — cut right here.
                self.cut(end)?;
                continue;
            }

            match self.quietest_frame {
                Some((_, best)) if best <= rms => {}
                _ => self.quietest_frame = Some((end, rms)),
            }
            self.scan_pos = end;
        }

        // Hard cap: cut at the quietest frame we saw, or at the cap itself.
        if self.buffer.len() >= MAX_SAMPLES {
            let at = self.quietest_frame.map(|(end, _)| end).unwrap_or(MAX_SAMPLES);
            self.cut(at)?;
        }

        Ok(())
    }

    /// Flush whatever remains as a final (possibly short) chunk.
    pub fn finalize(mut self) -> Result<Vec<ChunkSummary>, String> {
        if self.buffer.len() >= MIN_FINAL_SAMPLES {
            let at = self.buffer.len();
            self.cut(at)?;
        }
        Ok(self.written)
    }

    fn cut(&mut self, at: usize) -> Result<(), String> {
        let chunk: Vec<i16> = self.buffer.drain(..at).collect();
        let rms = super::rms_i16(&chunk);
        let silent = rms < SILENT_CHUNK_RMS;

        let filename = format!("chunk-{:04}-{}.wav", self.chunk_index, self.stream.as_str());
        let path = self.dir.join(&filename);
        write_wav(&path, &chunk)?;

        let summary = ChunkSummary {
            stream: self.stream,
            index: self.chunk_index,
            path: path.to_string_lossy().into_owned(),
            start_ms: self.buffer_start_sample * 1000 / SAMPLE_RATE as u64,
            duration_ms: chunk.len() as u64 * 1000 / SAMPLE_RATE as u64,
            silent,
        };
        if let Some(app) = &self.app {
            let _ = app.emit("recording://chunk", summary.clone());
        }
        if let Some(sink) = &mut self.sink {
            sink(&summary);
        }
        self.written.push(summary);

        self.buffer_start_sample += chunk.len() as u64;
        self.chunk_index += 1;
        self.quietest_frame = None;
        self.scan_pos = TARGET_SAMPLES;
        Ok(())
    }
}

fn write_wav(path: &Path, samples: &[i16]) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("create {path:?}: {e}"))?;
    let mut i16_writer = writer.get_i16_writer(samples.len() as u32);
    for &s in samples {
        i16_writer.write_sample(s);
    }
    i16_writer
        .flush()
        .map_err(|e| format!("write {path:?}: {e}"))?;
    writer.finalize().map_err(|e| format!("finalize {path:?}: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunker(dir: &Path) -> Chunker {
        Chunker::new(None, StreamKind::Mic, dir, 0, None)
    }

    fn loud(n: usize) -> Vec<i16> {
        // Alternating square wave well above every threshold.
        (0..n).map(|i| if i % 2 == 0 { 8000 } else { -8000 }).collect()
    }

    #[test]
    fn cuts_at_silence_after_target() {
        let dir = std::env::temp_dir().join("harknotes-chunker-test-1");
        std::fs::create_dir_all(&dir).unwrap();
        let mut c = chunker(&dir);

        // 46s of speech, then 1s of silence, then 2s of speech.
        c.push(&loud(46 * SAMPLE_RATE as usize)).unwrap();
        c.push(&vec![0i16; SAMPLE_RATE as usize]).unwrap();
        c.push(&loud(2 * SAMPLE_RATE as usize)).unwrap();
        let chunks = c.finalize().unwrap();

        assert_eq!(chunks.len(), 2);
        // First chunk cut at the first silent frame after 46s: 46s..46.2s.
        assert!(chunks[0].duration_ms >= 46_000 && chunks[0].duration_ms <= 46_400);
        assert!(!chunks[0].silent);
        assert_eq!(chunks[1].start_ms, chunks[0].duration_ms);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hard_caps_at_max_without_silence() {
        let dir = std::env::temp_dir().join("harknotes-chunker-test-2");
        std::fs::create_dir_all(&dir).unwrap();
        let mut c = chunker(&dir);

        c.push(&loud(61 * SAMPLE_RATE as usize)).unwrap();
        let chunks = c.finalize().unwrap();

        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].duration_ms <= 60_000);
        assert!(chunks[0].duration_ms >= 45_000);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn marks_silent_chunks() {
        let dir = std::env::temp_dir().join("harknotes-chunker-test-3");
        std::fs::create_dir_all(&dir).unwrap();
        let mut c = chunker(&dir);

        c.push(&vec![0i16; 2 * SAMPLE_RATE as usize]).unwrap();
        let chunks = c.finalize().unwrap();

        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].silent);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn drops_tiny_trailing_chunk() {
        let dir = std::env::temp_dir().join("harknotes-chunker-test-4");
        std::fs::create_dir_all(&dir).unwrap();
        let mut c = chunker(&dir);

        c.push(&loud(SAMPLE_RATE as usize / 4)).unwrap(); // 250ms
        let chunks = c.finalize().unwrap();

        assert!(chunks.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
