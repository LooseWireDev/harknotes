import { useEffect, useRef, useState } from 'react';
import { createFileRoute } from '@tanstack/react-router';

import {
  onChunk,
  onDuration,
  onLevel,
  onStreamError,
  recordingStatus,
  startRecording,
  stopRecording,
  type ChunkSummary,
  type StreamKind,
} from '../lib/recording';

export const Route = createFileRoute('/')({
  component: HomePage,
});

function formatDuration(totalSeconds: number): string {
  const m = Math.floor(totalSeconds / 60);
  const s = totalSeconds % 60;
  return `${m}:${String(s).padStart(2, '0')}`;
}

function HomePage(): React.ReactElement {
  const [recording, setRecording] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [levels, setLevels] = useState<Record<StreamKind, number>>({ mic: 0, system: 0 });
  const [chunks, setChunks] = useState<ChunkSummary[]>([]);
  const [errors, setErrors] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  // Decay levels toward zero so bars fall when a stream goes quiet.
  const decayTimer = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    void recordingStatus().then((status) => {
      if (!disposed) {
        setRecording(status.recording);
        setSeconds(status.durationSeconds);
      }
    });

    void Promise.all([
      onLevel((e) => setLevels((prev) => ({ ...prev, [e.stream]: e.rms }))),
      onDuration((e) => setSeconds(e.seconds)),
      onChunk((chunk) => setChunks((prev) => [...prev, chunk])),
      onStreamError((e) => setErrors((prev) => [...prev, `${e.stream}: ${e.message}`])),
    ]).then((fns) => {
      if (disposed) {
        for (const fn of fns) fn();
      } else {
        unlisteners.push(...fns);
      }
    });

    decayTimer.current = setInterval(() => {
      setLevels((prev) => ({ mic: prev.mic * 0.7, system: prev.system * 0.7 }));
    }, 250);

    return () => {
      disposed = true;
      for (const fn of unlisteners) fn();
      if (decayTimer.current) clearInterval(decayTimer.current);
    };
  }, []);

  const toggle = async (): Promise<void> => {
    setBusy(true);
    setErrors([]);
    try {
      if (recording) {
        await stopRecording();
        setRecording(false);
      } else {
        setChunks([]);
        setSeconds(0);
        await startRecording();
        setRecording(true);
      }
    } catch (err) {
      setErrors((prev) => [...prev, String(err)]);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto flex min-h-screen max-w-xl flex-col gap-6 p-8">
      <header className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Harknotes</h1>
        {recording && (
          <span className="font-mono text-lg tabular-nums text-red-500">
            ● {formatDuration(seconds)}
          </span>
        )}
      </header>

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

      {chunks.length > 0 && (
        <section className="text-sm">
          <h2 className="mb-1 font-medium text-neutral-500">Chunks written</h2>
          <ul className="flex flex-col gap-1 font-mono text-xs text-neutral-600">
            {chunks.map((c) => (
              <li key={`${c.stream}-${c.index}`}>
                [{c.stream}] #{c.index} · {formatDuration(Math.round(c.startMs / 1000))} +{' '}
                {(c.durationMs / 1000).toFixed(1)}s{c.silent ? ' · silent' : ''}
              </li>
            ))}
          </ul>
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
