import { useState } from 'react';
import { createFileRoute, useNavigate } from '@tanstack/react-router';
import { useQuery, useQueryClient } from '@tanstack/react-query';

import {
  deleteMeeting,
  exportMeeting,
  formatTimestamp,
  getMeeting,
  getTranscript,
  renameMeeting,
} from '../lib/transcription';
import {
  getAiSettings,
  saveSummary,
  summarize,
  SummarySchema,
  type Summary,
} from '../lib/summarize';

export const Route = createFileRoute('/meeting/$meetingId')({
  component: MeetingPage,
});

function MeetingPage(): React.ReactElement {
  const { meetingId } = Route.useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [editingTitle, setEditingTitle] = useState<string | null>(null);
  const [exportedTo, setExportedTo] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [summarizing, setSummarizing] = useState(false);

  const { data: meeting } = useQuery({
    queryKey: ['meeting', meetingId],
    queryFn: () => getMeeting(meetingId),
  });
  const { data: transcript } = useQuery({
    queryKey: ['transcript', meetingId],
    queryFn: () => getTranscript(meetingId),
    // Refresh while chunks are still being transcribed.
    refetchInterval: meeting?.status === 'ready' ? false : 3000,
  });

  const saveTitle = async (): Promise<void> => {
    if (editingTitle === null || !meeting) return;
    const next = editingTitle.trim();
    setEditingTitle(null);
    if (!next || next === meeting.title) return;
    try {
      await renameMeeting(meetingId, next);
      await queryClient.invalidateQueries({ queryKey: ['meeting', meetingId] });
      await queryClient.invalidateQueries({ queryKey: ['meetings'] });
    } catch (err) {
      setError(String(err));
    }
  };

  const remove = async (): Promise<void> => {
    if (!window.confirm('Delete this meeting and its recordings?')) return;
    try {
      await deleteMeeting(meetingId);
      await queryClient.invalidateQueries({ queryKey: ['meetings'] });
      void navigate({ to: '/' });
    } catch (err) {
      setError(String(err));
    }
  };

  const doExport = async (): Promise<void> => {
    try {
      setExportedTo(await exportMeeting(meetingId));
    } catch (err) {
      setError(String(err));
    }
  };

  const doSummarize = async (): Promise<void> => {
    if (!meeting || !transcript || transcript.length === 0) return;
    setSummarizing(true);
    setError(null);
    try {
      const settings = await getAiSettings();
      const summary = await summarize(
        settings,
        transcript,
        Math.max(1, Math.round(meeting.durationSeconds / 60)),
      );
      await saveSummary(meetingId, summary);
      await queryClient.invalidateQueries({ queryKey: ['meeting', meetingId] });
    } catch (err) {
      setError(`Summarization failed: ${String(err)}`);
    } finally {
      setSummarizing(false);
    }
  };

  const parsedSummary: Summary | null = (() => {
    if (!meeting?.summaryJson) return null;
    try {
      const result = SummarySchema.safeParse(JSON.parse(meeting.summaryJson));
      return result.success ? result.data : null;
    } catch {
      return null;
    }
  })();

  if (!meeting) {
    return <div className="p-8 text-neutral-400">Loading…</div>;
  }

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-8">
      <header className="flex items-start justify-between gap-4">
        {editingTitle === null ? (
          <button
            type="button"
            onClick={() => setEditingTitle(meeting.title)}
            className="text-left text-2xl font-semibold hover:text-neutral-600"
            title="Click to rename"
          >
            {meeting.title}
          </button>
        ) : (
          <input
            autoFocus
            value={editingTitle}
            onChange={(e) => setEditingTitle(e.target.value)}
            onBlur={() => void saveTitle()}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void saveTitle();
              if (e.key === 'Escape') setEditingTitle(null);
            }}
            className="flex-1 rounded-md border border-neutral-300 px-2 py-1 text-2xl font-semibold"
          />
        )}
        <div className="flex shrink-0 gap-2">
          <button
            type="button"
            onClick={() => void doSummarize()}
            disabled={summarizing || meeting.status !== 'ready' || !transcript?.length}
            className="rounded-md bg-violet-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-violet-700 disabled:opacity-50"
          >
            {summarizing ? 'Summarizing…' : parsedSummary ? 'Re-summarize' : 'Summarize'}
          </button>
          <button
            type="button"
            onClick={() => void doExport()}
            className="rounded-md border border-neutral-300 px-3 py-1.5 text-sm hover:bg-neutral-50"
          >
            Export
          </button>
          <button
            type="button"
            onClick={() => void remove()}
            className="rounded-md border border-red-200 px-3 py-1.5 text-sm text-red-600 hover:bg-red-50"
          >
            Delete
          </button>
        </div>
      </header>

      <p className="text-sm text-neutral-400">
        {meeting.createdAt} · {formatTimestamp(meeting.durationSeconds * 1000)}
        {meeting.status !== 'ready' && ' · transcribing…'}
      </p>

      {error && (
        <div className="rounded-md border border-red-300 bg-red-50 p-3 text-sm text-red-800">
          {error}
        </div>
      )}
      {exportedTo && (
        <div className="rounded-md border border-emerald-300 bg-emerald-50 p-3 text-sm text-emerald-800">
          Exported to {exportedTo}
        </div>
      )}

      {parsedSummary && <SummaryView summary={parsedSummary} />}

      <section>
        <h2 className="mb-2 text-sm font-medium text-neutral-500">Transcript</h2>
        {!transcript || transcript.length === 0 ? (
          <p className="text-sm text-neutral-400">
            {meeting.status === 'ready' ? 'No speech detected.' : 'Transcribing…'}
          </p>
        ) : (
          <div className="flex flex-col gap-1.5 text-sm">
            {transcript.map((s) => (
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
          </div>
        )}
      </section>
    </div>
  );
}

function SummaryView({ summary }: { summary: Summary }): React.ReactElement {
  return (
    <section className="flex flex-col gap-4 rounded-lg border border-violet-200 bg-violet-50/50 p-4">
      <div>
        <h2 className="mb-1 text-sm font-medium text-violet-700">Summary</h2>
        <p className="text-sm">{summary.summary}</p>
      </div>

      {summary.keyTopics.length > 0 && (
        <div>
          <h3 className="mb-1 text-sm font-medium text-violet-700">Key topics</h3>
          <ul className="flex flex-col gap-1.5 text-sm">
            {summary.keyTopics.map((t) => (
              <li key={t.topic}>
                <span className="font-medium">{t.topic}:</span> {t.detail}
              </li>
            ))}
          </ul>
        </div>
      )}

      {summary.actionItems.length > 0 && (
        <div>
          <h3 className="mb-1 text-sm font-medium text-violet-700">Action items</h3>
          <ul className="list-inside list-disc text-sm">
            {summary.actionItems.map((a) => (
              <li key={`${a.speaker}-${a.action}`}>
                <span className="font-medium">{a.speaker}</span> — {a.action}
              </li>
            ))}
          </ul>
        </div>
      )}

      {summary.decisions.length > 0 && (
        <div>
          <h3 className="mb-1 text-sm font-medium text-violet-700">Decisions</h3>
          <ul className="list-inside list-disc text-sm">
            {summary.decisions.map((d) => (
              <li key={d}>{d}</li>
            ))}
          </ul>
        </div>
      )}

      {summary.openQuestions.length > 0 && (
        <div>
          <h3 className="mb-1 text-sm font-medium text-violet-700">Open questions</h3>
          <ul className="list-inside list-disc text-sm">
            {summary.openQuestions.map((q) => (
              <li key={q}>{q}</li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
