import { useRef, useState } from 'react';
import { createFileRoute, useNavigate } from '@tanstack/react-router';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Check, Download, Plus, Sparkles, Trash2, X } from 'lucide-react';

import { EditableText } from '@/components/EditableText';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  getAiSettings,
  saveSummary,
  summarize,
  SummarySchema,
  type Summary,
} from '@/lib/summarize';
import {
  deleteMeeting,
  exportMeeting,
  formatTimestamp,
  getMeeting,
  getTranscript,
  renameMeeting,
  renameSpeaker,
  setNotes,
  setTags,
  updateSegment,
} from '@/lib/transcription';

export const Route = createFileRoute('/meeting/$meetingId')({
  component: MeetingPage,
});

const SPEAKER_COLORS = ['text-mint', 'text-sky-400', 'text-violet-400', 'text-amber-400'];

function speakerColor(speaker: string, order: string[]): string {
  if (speaker === 'User') return 'text-mint';
  const idx = order.indexOf(speaker);
  return SPEAKER_COLORS[(idx < 0 ? 1 : idx + 1) % SPEAKER_COLORS.length] ?? 'text-text-secondary';
}

function MeetingPage(): React.ReactElement {
  const { meetingId } = Route.useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [exportedTo, setExportedTo] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [summarizing, setSummarizing] = useState(false);
  const [renamingSpeaker, setRenamingSpeaker] = useState<string | null>(null);
  const [newTag, setNewTag] = useState<string | null>(null);
  const noteSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const { data: meeting } = useQuery({
    queryKey: ['meeting', meetingId],
    queryFn: () => getMeeting(meetingId),
  });
  const { data: transcript } = useQuery({
    queryKey: ['transcript', meetingId],
    queryFn: () => getTranscript(meetingId),
    refetchInterval: meeting?.status === 'ready' ? false : 3000,
  });

  const invalidate = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ['meeting', meetingId] });
    await queryClient.invalidateQueries({ queryKey: ['meetings'] });
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

  const patchSummary = async (patch: Partial<Summary>): Promise<void> => {
    if (!parsedSummary) return;
    try {
      await saveSummary(meetingId, { ...parsedSummary, ...patch });
      await invalidate();
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
        meeting.notes,
      );
      await saveSummary(meetingId, summary);
      await invalidate();
    } catch (err) {
      setError(`Summarization failed: ${String(err)}`);
    } finally {
      setSummarizing(false);
    }
  };

  const saveNotesDebounced = (value: string): void => {
    // Optimistic cache update so typing stays smooth.
    queryClient.setQueryData(['meeting', meetingId], (prev: typeof meeting) =>
      prev ? { ...prev, notes: value } : prev,
    );
    if (noteSaveTimer.current) clearTimeout(noteSaveTimer.current);
    noteSaveTimer.current = setTimeout(() => void setNotes(meetingId, value), 800);
  };

  const editSegment = async (segmentId: number | undefined, text: string): Promise<void> => {
    if (segmentId === undefined) return;
    try {
      await updateSegment(segmentId, text);
      await queryClient.invalidateQueries({ queryKey: ['transcript', meetingId] });
    } catch (err) {
      setError(String(err));
    }
  };

  const doRenameSpeaker = async (from: string, to: string): Promise<void> => {
    setRenamingSpeaker(null);
    if (!to.trim() || to === from) return;
    try {
      await renameSpeaker(meetingId, from, to.trim());
      await queryClient.invalidateQueries({ queryKey: ['transcript', meetingId] });
    } catch (err) {
      setError(String(err));
    }
  };

  const updateTags = async (tags: string[]): Promise<void> => {
    try {
      await setTags(meetingId, tags);
      await invalidate();
    } catch (err) {
      setError(String(err));
    }
  };

  if (!meeting) {
    return <div className="p-8 text-text-tertiary">Loading…</div>;
  }

  const speakerOrder = [...new Set((transcript ?? []).map((s) => s.speaker))];

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5 p-6">
      {/* Header */}
      <header className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <EditableText
            value={meeting.title}
            onSave={(t) => void renameMeeting(meetingId, t).then(invalidate)}
            className="text-2xl font-semibold tracking-tight"
          />
          <p className="mt-1 text-sm text-text-secondary">
            {meeting.createdAt} · {formatTimestamp(meeting.durationSeconds * 1000)}
            {meeting.status !== 'ready' && (
              <span className="ml-2 text-warning">transcribing…</span>
            )}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button
            size="sm"
            disabled={summarizing || meeting.status !== 'ready' || !transcript?.length}
            onClick={() => void doSummarize()}
          >
            <Sparkles /> {summarizing ? 'Summarizing…' : parsedSummary ? 'Re-summarize' : 'Summarize'}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void exportMeeting(meetingId).then(setExportedTo).catch((e) => setError(String(e)))}
          >
            <Download /> Export
          </Button>
          {confirmDelete ? (
            <>
              <span className="text-xs text-text-secondary">Delete?</span>
              <Button variant="ghost" size="icon-sm" onClick={() => setConfirmDelete(false)}>
                <X />
              </Button>
              <Button
                variant="destructive"
                size="icon-sm"
                onClick={() =>
                  void deleteMeeting(meetingId)
                    .then(invalidate)
                    .then(() => navigate({ to: '/' }))
                }
              >
                <Check />
              </Button>
            </>
          ) : (
            <Button variant="ghost" size="icon-sm" onClick={() => setConfirmDelete(true)}>
              <Trash2 />
            </Button>
          )}
        </div>
      </header>

      {/* Tags */}
      <div className="flex flex-wrap items-center gap-1.5">
        {meeting.tags.map((tag) => (
          <Badge key={tag} variant="neutral" className="group gap-1">
            {tag}
            <button
              type="button"
              className="opacity-0 transition-opacity group-hover:opacity-100"
              onClick={() => void updateTags(meeting.tags.filter((t) => t !== tag))}
            >
              <X className="size-3" />
            </button>
          </Badge>
        ))}
        {newTag === null ? (
          <Button variant="ghost" size="xs" onClick={() => setNewTag('')}>
            <Plus /> tag
          </Button>
        ) : (
          <input
            autoFocus
            value={newTag}
            onChange={(e) => setNewTag(e.target.value)}
            onBlur={() => setNewTag(null)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && newTag.trim()) {
                void updateTags([...meeting.tags, newTag.trim()]);
                setNewTag(null);
              }
              if (e.key === 'Escape') setNewTag(null);
            }}
            placeholder="tag name"
            className="h-6 w-24 rounded-md border border-border bg-transparent px-2 text-xs outline-none focus:border-mint/50"
          />
        )}
      </div>

      {error && (
        <div className="rounded-lg border border-record-red/30 bg-record-red-dim px-4 py-3 text-sm text-record-red">
          {error}
        </div>
      )}
      {exportedTo && (
        <div className="rounded-lg border border-mint/30 bg-mint-subtle px-4 py-3 text-sm text-mint">
          Exported to {exportedTo}
        </div>
      )}

      {/* My notes */}
      <section className="rounded-xl border border-border-subtle bg-surface-1 p-4">
        <h2 className="mb-2 text-xs font-medium uppercase tracking-wide text-text-tertiary">
          My notes
        </h2>
        <textarea
          value={meeting.notes}
          onChange={(e) => saveNotesDebounced(e.target.value)}
          placeholder="Your own notes — used to shape the AI summary."
          rows={Math.max(2, meeting.notes.split('\n').length)}
          className="w-full resize-none bg-transparent text-sm leading-relaxed outline-none placeholder:text-text-tertiary"
        />
      </section>

      {/* Summary */}
      {parsedSummary && (
        <section className="flex flex-col gap-4 rounded-xl border border-mint/20 bg-mint-glow p-4">
          <div>
            <h2 className="mb-1.5 text-xs font-medium uppercase tracking-wide text-mint">Summary</h2>
            <EditableText
              multiline
              value={parsedSummary.summary}
              onSave={(v) => void patchSummary({ summary: v })}
              className="text-sm leading-relaxed text-text-primary"
            />
          </div>

          {parsedSummary.keyTopics.length > 0 && (
            <div>
              <h3 className="mb-1.5 text-xs font-medium uppercase tracking-wide text-mint">
                Key topics
              </h3>
              <ul className="flex flex-col gap-1.5 text-sm">
                {parsedSummary.keyTopics.map((t, i) => (
                  <li key={t.topic}>
                    <span className="font-medium">{t.topic}:</span>{' '}
                    <EditableText
                      multiline
                      value={t.detail}
                      onSave={(v) =>
                        void patchSummary({
                          keyTopics: parsedSummary.keyTopics.map((kt, j) =>
                            j === i ? { ...kt, detail: v } : kt,
                          ),
                        })
                      }
                      className="text-text-secondary"
                    />
                  </li>
                ))}
              </ul>
            </div>
          )}

          {parsedSummary.actionItems.length > 0 && (
            <div>
              <h3 className="mb-1.5 text-xs font-medium uppercase tracking-wide text-mint">
                Action items
              </h3>
              <ul className="flex flex-col gap-1 text-sm">
                {parsedSummary.actionItems.map((a, i) => (
                  <li key={`${a.speaker}-${a.action}`} className="flex items-start gap-2">
                    <input
                      type="checkbox"
                      checked={a.done ?? false}
                      onChange={(e) =>
                        void patchSummary({
                          actionItems: parsedSummary.actionItems.map((ai, j) =>
                            j === i ? { ...ai, done: e.target.checked } : ai,
                          ),
                        })
                      }
                      className="mt-0.5 size-4 accent-[var(--mint)]"
                    />
                    <span className={a.done ? 'text-text-tertiary line-through' : ''}>
                      <span className="font-medium">{a.speaker}</span> —{' '}
                      <EditableText
                        value={a.action}
                        onSave={(v) =>
                          void patchSummary({
                            actionItems: parsedSummary.actionItems.map((ai, j) =>
                              j === i ? { ...ai, action: v } : ai,
                            ),
                          })
                        }
                      />
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {(['decisions', 'openQuestions'] as const).map((key) =>
            parsedSummary[key].length > 0 ? (
              <div key={key}>
                <h3 className="mb-1.5 text-xs font-medium uppercase tracking-wide text-mint">
                  {key === 'decisions' ? 'Decisions' : 'Open questions'}
                </h3>
                <ul className="flex list-inside list-disc flex-col gap-1 text-sm">
                  {parsedSummary[key].map((item, i) => (
                    <li key={item}>
                      <EditableText
                        value={item}
                        onSave={(v) =>
                          void patchSummary({
                            [key]: parsedSummary[key].map((x, j) => (j === i ? v : x)),
                          })
                        }
                      />
                    </li>
                  ))}
                </ul>
              </div>
            ) : null,
          )}
        </section>
      )}

      {/* Transcript */}
      <section className="rounded-xl border border-border-subtle bg-surface-1 p-4">
        <h2 className="mb-2 text-xs font-medium uppercase tracking-wide text-text-tertiary">
          Transcript
        </h2>
        {!transcript || transcript.length === 0 ? (
          <p className="text-sm text-text-tertiary">
            {meeting.status === 'ready' ? 'No speech detected.' : 'Transcribing…'}
          </p>
        ) : (
          <div className="flex flex-col gap-1.5 text-sm">
            {transcript.map((s) => (
              <p key={s.id ?? `${s.speaker}-${s.startMs}`} className="leading-relaxed">
                <span className="font-mono text-xs text-text-tertiary">
                  [{formatTimestamp(s.startMs)}]
                </span>{' '}
                {renamingSpeaker === s.speaker ? (
                  <input
                    autoFocus
                    defaultValue={s.speaker}
                    onBlur={(e) => void doRenameSpeaker(s.speaker, e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') void doRenameSpeaker(s.speaker, e.currentTarget.value);
                      if (e.key === 'Escape') setRenamingSpeaker(null);
                    }}
                    className="w-28 rounded-md border border-border bg-transparent px-1.5 text-sm outline-none focus:border-mint/50"
                  />
                ) : (
                  <button
                    type="button"
                    title="Click to rename this speaker everywhere"
                    onClick={() => setRenamingSpeaker(s.speaker)}
                    className={`font-medium hover:underline ${speakerColor(s.speaker, speakerOrder)}`}
                  >
                    {s.speaker}:
                  </button>
                )}{' '}
                <EditableText
                  multiline
                  value={s.text}
                  onSave={(v) => void editSegment(s.id, v)}
                  className="text-text-primary"
                />
              </p>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
