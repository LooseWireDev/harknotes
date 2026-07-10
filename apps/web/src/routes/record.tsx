import { useEffect, useRef, useState } from 'react';
import { createFileRoute, Link } from '@tanstack/react-router';

import {
  onChunk,
  onDuration,
  onLevel,
  onStreamError,
  recordingStatus,
  startRecording,
  stopRecording,
  type StreamKind,
} from '../lib/recording';
import {
  downloadModel,
  formatTimestamp,
  getWhisperModel,
  listModels,
  onChunkDone,
  onChunkFailed,
  onMeetingReady,
  onModelProgress,
  setWhisperModel,
  type ModelInfo,
  type Segment,
} from '../lib/transcription';

export const Route = createFileRoute('/record')({
  component: RecordPage,
});

function formatDuration(totalSeconds: number): string {
  const m = Math.floor(totalSeconds / 60);
  const s = totalSeconds % 60;
  return `${m}:${String(s).padStart(2, '0')}`;
}

function RecordPage(): React.ReactElement {
  const [recording, setRecording] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [levels, setLevels] = useState<Record<StreamKind, number>>({ mic: 0, system: 0 });
  const [segments, setSegments] = useState<Segment[]>([]);
  const [errors, setErrors] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [ready, setReady] = useState<string | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [model, setModel] = useState('base');
  const [downloadPct, setDownloadPct] = useState<number | null>(null);
  const transcriptEnd = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    void recordingStatus().then((status) => {
      if (!disposed) {
        setRecording(status.recording);
        setSeconds(status.durationSeconds);
      }
    });
    void listModels().then((m) => !disposed && setModels(m));
    void getWhisperModel().then((m) => !disposed && setModel(m));

    void Promise.all([
      onLevel((e) => setLevels((prev) => ({ ...prev, [e.stream]: e.rms }))),
      onDuration((e) => setSeconds(e.seconds)),
      onChunk(() => {}),
      onStreamError((e) => setErrors((prev) => [...prev, `${e.stream}: ${e.message}`])),
      onChunkDone((e) =>
        setSegments((prev) =>
          [...prev, ...e.segments].sort((a, b) => a.startMs - b.startMs),
        ),
      ),
      onChunkFailed((e) =>
        setErrors((prev) => [...prev, `transcribe ${e.stream}#${e.idx}: ${e.error}`]),
      ),
      onMeetingReady((e) => setReady(e.meetingId)),
      onModelProgress((e) => {
        if (e.totalBytes > 0 && e.downloadedBytes >= e.totalBytes) {
          setDownloadPct(null);
          void listModels().then(setModels);
        } else {
          setDownloadPct(
            e.totalBytes > 0 ? Math.round((e.downloadedBytes / e.totalBytes) * 100) : 0,
          );
        }
      }),
    ]).then((fns) => {
      if (disposed) {
        for (const fn of fns) fn();
      } else {
        unlisteners.push(...fns);
      }
    });

    const decay = setInterval(() => {
      setLevels((prev) => ({ mic: prev.mic * 0.7, system: prev.system * 0.7 }));
    }, 250);

    return () => {
      disposed = true;
      for (const fn of unlisteners) fn();
      clearInterval(decay);
    };
  }, []);

  useEffect(() => {
    transcriptEnd.current?.scrollIntoView({ behavior: 'smooth' });
  }, [segments.length]);

  const toggle = async (): Promise<void> => {
    setBusy(true);
    setErrors([]);
    try {
      if (recording) {
        await stopRecording();
        setRecording(false);
      } else {
        setSegments([]);
        setSeconds(0);
        setReady(null);
        await startRecording();
        setRecording(true);
      }
    } catch (err) {
      setErrors((prev) => [...prev, String(err)]);
    } finally {
      setBusy(false);
    }
  };

  const selectedModel = models.find((m) => m.name === model);

  const changeModel = async (name: string): Promise<void> => {
    setModel(name);
    await setWhisperModel(name);
  };

  return (
    <div className="mx-auto flex min-h-screen max-w-2xl flex-col gap-5 p-8">
      <header className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Record</h1>
        {recording && (
          <span className="font-mono text-lg tabular-nums text-red-500">
            ● {formatDuration(seconds)}
          </span>
        )}
      </header>

      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={() => void toggle()}
          disabled={busy}
          className={`rounded-lg px-6 py-3 text-lg font-medium text-white transition-colors disabled:opacity-50 ${
            recording ? 'bg-red-600 hover:bg-red-700' : 'bg-emerald-600 hover:bg-emerald-700'
          }`}
        >
          {recording ? 'Stop recording' : 'Start recording'}
        </button>

        <label className="ml-auto flex items-center gap-2 text-sm text-neutral-500">
          Model
          <select
            value={model}
            onChange={(e) => void changeModel(e.target.value)}
            disabled={recording}
            className="rounded-md border border-neutral-300 bg-white px-2 py-1 text-sm text-neutral-800"
          >
            {models.map((m) => (
              <option key={m.name} value={m.name}>
                {m.name} ({m.sizeMb} MB){m.downloaded ? ' ✓' : ''}
              </option>
            ))}
          </select>
        </label>
      </div>

      {selectedModel && !selectedModel.downloaded && (
        <div className="flex items-center gap-3 rounded-md border border-amber-300 bg-amber-50 p-3 text-sm text-amber-800">
          {downloadPct === null ? (
            <>
              <span>
                Model “{selectedModel.name}” isn’t downloaded yet ({selectedModel.sizeMb} MB).
                It will download automatically on first use, or:
              </span>
              <button
                type="button"
                onClick={() => void downloadModel(selectedModel.name)}
                className="rounded bg-amber-600 px-3 py-1 font-medium text-white hover:bg-amber-700"
              >
                Download now
              </button>
            </>
          ) : (
            <span>Downloading {selectedModel.name}… {downloadPct}%</span>
          )}
        </div>
      )}

      <div className="flex flex-col gap-2">
        <LevelBar label="Mic" value={levels.mic} />
        <LevelBar label="System" value={levels.system} />
      </div>

      {errors.length > 0 && (
        <div className="rounded-md border border-red-300 bg-red-50 p-3 text-sm text-red-800">
          {errors.map((e) => (
            <p key={e}>{e}</p>
          ))}
        </div>
      )}

      {ready && (
        <div className="flex items-center justify-between rounded-md border border-emerald-300 bg-emerald-50 p-3 text-sm text-emerald-800">
          <span>Transcription complete.</span>
          <Link
            to="/meeting/$meetingId"
            params={{ meetingId: ready }}
            className="font-medium underline hover:no-underline"
          >
            Open meeting →
          </Link>
        </div>
      )}

      {(segments.length > 0 || recording) && (
        <section className="flex min-h-0 flex-1 flex-col">
          <h2 className="mb-2 text-sm font-medium text-neutral-500">
            {recording ? 'Live transcript' : 'Transcript'}
          </h2>
          <div className="max-h-96 flex-1 overflow-y-auto rounded-md border border-neutral-200 bg-neutral-50 p-3">
            {segments.length === 0 ? (
              <p className="text-sm text-neutral-400">
                Transcript appears here as chunks are processed…
              </p>
            ) : (
              <div className="flex flex-col gap-1.5 text-sm">
                {segments.map((s) => (
                  <p key={`${s.speaker}-${s.startMs}-${s.endMs}`}>
                    <span className="font-mono text-xs text-neutral-400">
                      [{formatTimestamp(s.startMs)}]
                    </span>{' '}
                    <span
                      className={
                        s.speaker === 'User'
                          ? 'font-medium text-emerald-700'
                          : 'font-medium text-sky-700'
                      }
                    >
                      {s.speaker}:
                    </span>{' '}
                    {s.text}
                  </p>
                ))}
                <div ref={transcriptEnd} />
              </div>
            )}
          </div>
        </section>
      )}
    </div>
  );
}

function LevelBar({ label, value }: { label: string; value: number }): React.ReactElement {
  // RMS of speech rarely exceeds ~0.3; scale so normal speech fills the bar.
  const pct = Math.min(100, Math.sqrt(Math.min(value * 3, 1)) * 100);
  return (
    <div className="flex items-center gap-3">
      <span className="w-16 text-sm text-neutral-500">{label}</span>
      <div className="h-2 flex-1 overflow-hidden rounded-full bg-neutral-200">
        <div
          className="h-full rounded-full bg-emerald-500 transition-[width] duration-100"
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
