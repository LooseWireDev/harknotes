import { useEffect, useRef, useState } from 'react';
import { createFileRoute, Link } from '@tanstack/react-router';
import { Mic, Square } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { WaveformVisualizer } from '@/components/WaveformVisualizer';
import {
  onDuration,
  onLevel,
  onStreamError,
  recordingStatus,
  startRecording,
  stopRecording,
} from '@/lib/recording';
import {
  downloadModel,
  formatTimestamp,
  getWhisperModel,
  listModels,
  onChunkDone,
  onChunkFailed,
  onMeetingReady,
  onModelProgress,
  setNotes,
  type ModelInfo,
  type Segment,
} from '@/lib/transcription';

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
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [seconds, setSeconds] = useState(0);
  const [segments, setSegments] = useState<Segment[]>([]);
  const [errors, setErrors] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [ready, setReady] = useState<string | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [model, setModel] = useState('base');
  const [downloadPct, setDownloadPct] = useState<number | null>(null);
  const [noteDraft, setNoteDraft] = useState('');

  const micLevelRef = useRef(0);
  const systemLevelRef = useRef(0);
  const transcriptEnd = useRef<HTMLDivElement | null>(null);
  const noteSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const meetingIdRef = useRef<string | null>(null);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    void recordingStatus().then((status) => {
      if (!disposed) {
        setRecording(status.recording);
        setMeetingId(status.meetingId);
        meetingIdRef.current = status.meetingId;
        setSeconds(status.durationSeconds);
      }
    });
    void listModels().then((m) => !disposed && setModels(m));
    void getWhisperModel().then((m) => !disposed && setModel(m));

    void Promise.all([
      onLevel((e) => {
        if (e.stream === 'mic') micLevelRef.current = e.rms;
        else systemLevelRef.current = e.rms;
      }),
      onDuration((e) => setSeconds(e.seconds)),
      onStreamError((e) => setErrors((prev) => [...prev, `${e.stream}: ${e.message}`])),
      onChunkDone((e) =>
        setSegments((prev) => [...prev, ...e.segments].sort((a, b) => a.startMs - b.startMs)),
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
          setDownloadPct(e.totalBytes > 0 ? Math.round((e.downloadedBytes / e.totalBytes) * 100) : 0);
        }
      }),
    ]).then((fns) => {
      if (disposed) for (const fn of fns) fn();
      else unlisteners.push(...fns);
    });

    return () => {
      disposed = true;
      for (const fn of unlisteners) fn();
    };
  }, []);

  useEffect(() => {
    transcriptEnd.current?.scrollIntoView({ behavior: 'smooth' });
  }, [segments.length]);

  const saveNotesDebounced = (value: string): void => {
    setNoteDraft(value);
    if (noteSaveTimer.current) clearTimeout(noteSaveTimer.current);
    noteSaveTimer.current = setTimeout(() => {
      const id = meetingIdRef.current;
      if (id) void setNotes(id, value);
    }, 800);
  };

  const toggle = async (): Promise<void> => {
    setBusy(true);
    setErrors([]);
    try {
      if (recording) {
        // Flush notes before stopping so nothing is lost.
        if (noteSaveTimer.current) clearTimeout(noteSaveTimer.current);
        if (meetingIdRef.current && noteDraft) {
          await setNotes(meetingIdRef.current, noteDraft);
        }
        await stopRecording();
        setRecording(false);
      } else {
        setSegments([]);
        setSeconds(0);
        setReady(null);
        setNoteDraft('');
        const started = await startRecording();
        setMeetingId(started.meetingId);
        meetingIdRef.current = started.meetingId;
        setRecording(true);
      }
    } catch (err) {
      setErrors((prev) => [...prev, String(err)]);
    } finally {
      setBusy(false);
    }
  };

  const selectedModel = models.find((m) => m.name === model);

  return (
    <div className="mx-auto flex h-[calc(100vh-53px)] max-w-4xl flex-col gap-4 p-6">
      {selectedModel && !selectedModel.downloaded && (
        <div className="flex items-center gap-3 rounded-lg border border-warning/30 bg-warning-dim px-4 py-3 text-sm text-warning">
          {downloadPct === null ? (
            <>
              <span className="flex-1">
                Model “{selectedModel.name}” isn’t downloaded ({selectedModel.sizeMb} MB). It
                downloads automatically on first use.
              </span>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void downloadModel(selectedModel.name)}
              >
                Download now
              </Button>
            </>
          ) : (
            <span className="flex flex-1 items-center gap-3">
              Downloading {selectedModel.name}…
              <span className="h-[5px] w-48 overflow-hidden rounded-full bg-surface-3">
                <span
                  className="block h-full rounded-full bg-gradient-to-r from-mint-dark to-mint transition-all duration-500 ease-out"
                  style={{ width: `${Math.max(2, downloadPct)}%` }}
                />
              </span>
              <span className="tabular-nums">{downloadPct}%</span>
            </span>
          )}
        </div>
      )}

      {errors.length > 0 && (
        <div className="rounded-lg border border-record-red/30 bg-record-red-dim px-4 py-3 text-sm text-record-red">
          {errors.map((e) => (
            <p key={e}>{e}</p>
          ))}
        </div>
      )}

      {ready && !recording && (
        <div className="flex items-center justify-between rounded-lg border border-mint/30 bg-mint-subtle px-4 py-3 text-sm">
          <span className="text-mint">Transcription complete.</span>
          <Link
            to="/meeting/$meetingId"
            params={{ meetingId: ready }}
            className="font-medium text-mint underline-offset-4 hover:underline"
          >
            Open meeting →
          </Link>
        </div>
      )}

      {recording ? (
        <div className="grid min-h-0 flex-1 grid-cols-1 gap-4 lg:grid-cols-2">
          {/* Left: live status */}
          <div className="flex min-h-0 flex-col gap-4">
            <div className="flex flex-col items-center justify-center gap-6 rounded-2xl border border-border-subtle bg-surface-1 py-10">
              <div className="flex flex-col items-center gap-2">
                <div className="flex items-center gap-2">
                  <div className="size-2 rounded-full bg-record-red animate-rec-blink" />
                  <span className="text-sm font-medium text-record-red">Recording</span>
                </div>
                <span className="font-mono text-4xl font-light tabular-nums tracking-[0.05em]">
                  {formatDuration(seconds)}
                </span>
              </div>
              <div className="flex items-center gap-6">
                <WaveformVisualizer levelRef={micLevelRef} color="#3dd68c" label="Mic" />
                <WaveformVisualizer levelRef={systemLevelRef} color="#8c939e" label="System" />
              </div>
              <Button variant="destructive" size="lg" disabled={busy} onClick={() => void toggle()}>
                <Square className="fill-current" /> Stop recording
              </Button>
            </div>

            <div className="flex min-h-0 flex-1 flex-col rounded-2xl border border-border-subtle bg-surface-1 p-4">
              <h2 className="mb-2 text-xs font-medium uppercase tracking-wide text-text-tertiary">
                Live transcript
              </h2>
              <div className="min-h-0 flex-1 overflow-y-auto">
                {segments.length === 0 ? (
                  <p className="text-sm text-text-tertiary">
                    Text appears here as ~45s chunks are transcribed…
                  </p>
                ) : (
                  <div className="flex flex-col gap-1.5 text-sm">
                    {segments.map((s) => (
                      <p key={`${s.speaker}-${s.startMs}-${s.endMs}`}>
                        <span className="font-mono text-xs text-text-tertiary">
                          [{formatTimestamp(s.startMs)}]
                        </span>{' '}
                        <span
                          className={
                            s.speaker === 'User'
                              ? 'font-medium text-mint'
                              : 'font-medium text-text-secondary'
                          }
                        >
                          {s.speaker}:
                        </span>{' '}
                        <span className="text-text-primary">{s.text}</span>
                      </p>
                    ))}
                    <div ref={transcriptEnd} />
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Right: my notes */}
          <div className="flex min-h-0 flex-col rounded-2xl border border-border-subtle bg-surface-1 p-4">
            <h2 className="mb-2 text-xs font-medium uppercase tracking-wide text-text-tertiary">
              My notes
            </h2>
            <textarea
              value={noteDraft}
              onChange={(e) => saveNotesDebounced(e.target.value)}
              placeholder="Type your own notes here — they're saved with the meeting and used to shape the AI summary…"
              className="min-h-0 flex-1 resize-none bg-transparent text-sm leading-relaxed text-text-primary outline-none placeholder:text-text-tertiary"
            />
          </div>
        </div>
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-6">
          <div className="flex size-24 items-center justify-center rounded-full bg-mint-subtle">
            <Mic className="size-10 text-mint" />
          </div>
          <div className="text-center">
            <h1 className="text-xl font-semibold">Ready to record</h1>
            <p className="mt-1 text-sm text-text-secondary">
              Mic and system audio are captured locally and transcribed on this machine.
            </p>
          </div>
          <Button size="lg" disabled={busy} onClick={() => void toggle()}>
            <Mic /> Start recording
          </Button>
        </div>
      )}
    </div>
  );
}
