# Harknotes

Local-first meeting notes for Linux (macOS planned). A privacy-focused
[Granola](https://granola.ai) alternative: no meeting bot, no cloud account,
**audio never leaves your machine**.

- **Silent capture** — records your mic and system audio (the other side of the
  call) directly from PipeWire/PulseAudio. No bot joins the meeting.
- **Live local transcription** — whisper.cpp transcribes ~45s chunks *during*
  the recording, one bounded process at a time, so the transcript is ready
  moments after the meeting ends — even on weak hardware.
- **Crash-proof** — every chunk is persisted and resumable; the in-progress
  buffer is snapshotted every 5s. A hard crash loses at most ~5 seconds of audio.
- **Local or BYOK summaries** — structured summaries (topics, action items,
  decisions, open questions) via any OpenAI-compatible endpoint: Ollama /
  LM Studio / llama.cpp locally, or your own OpenRouter/Groq/OpenAI key.
  Only transcript text is ever sent — and only if you ask for a summary.
- **Your data** — everything lives in a local SQLite DB; one-click markdown
  export.

## Stack

Nx + pnpm monorepo (scaffolded by the forge):

- `apps/web` — React 19 + Vite + TanStack Router/Query + Tailwind 4 (the UI)
- `apps/desktop` — Tauri v2. All native concerns in Rust: audio capture
  (`parec` monitor + `cpal` mic), silence-boundary chunker, whisper sidecar
  worker, SQLite (rusqlite), markdown export, tray
- `packages/*` — shared config/types

## Development

System deps (Fedora):

```bash
sudo dnf install -y webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel \
  librsvg2-devel gtk3-devel alsa-lib-devel gcc-c++ cmake
```

Build the whisper.cpp sidecar (once per machine — it's gitignored):

```bash
bash apps/desktop/scripts/build-whisper.sh
```

Run:

```bash
pnpm install
cd apps/desktop && pnpm tauri dev
```

Test:

```bash
cd apps/desktop/src-tauri && cargo test   # Rust: chunker, resampler, db, export
cd apps/web && pnpm test                  # TS: summarizer parsing/coercion
```

Package (deb/rpm/AppImage):

```bash
cd apps/desktop && pnpm tauri build
```

## Architecture notes

- **Dual-stream**: mic → "User" segments, system-audio monitor → "Meeting"
  segments, merged by absolute timestamp. Whisper models auto-download from
  Hugging Face on first use (tiny/base/small/medium/large-v3-turbo).
- **Silence gate**: chunks below an RMS floor are never transcribed — this is
  what kills whisper's "thank you for watching" hallucinations on dead air.
- **One whisper at a time**: the worker is a single queue with `nice -n 10`,
  a thread cap, a hard per-chunk timeout, and one retry. Failed chunks are
  retried on next launch.

## Status

Linux is fully working (capture → live transcription → notes → summaries →
export); deb/rpm bundles build at ~6MB. Known gaps: AppImage bundling fails
under linuxdeploy on Fedora (deb/rpm unaffected), and macOS capture
(ScreenCaptureKit) is the next platform target.
