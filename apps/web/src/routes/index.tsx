import { useEffect, useRef, useState } from 'react';
import { createFileRoute, Link, useNavigate } from '@tanstack/react-router';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Check, Mic, Pencil, Search, Trash2, X } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  deleteMeeting,
  listMeetings,
  onMeetingReady,
  renameMeeting,
  searchMeetings,
  type Meeting,
  type SearchResult,
} from '@/lib/transcription';

export const Route = createFileRoute('/')({
  component: MeetingsPage,
});

function formatDuration(totalSeconds: number): string {
  if (totalSeconds < 60) return `${totalSeconds}s`;
  return `${Math.floor(totalSeconds / 60)}m`;
}

function MeetingCard({ meeting, onChanged }: { meeting: Meeting; onChanged: () => void }): React.ReactElement {
  const navigate = useNavigate();
  const [editing, setEditing] = useState(false);
  const [editTitle, setEditTitle] = useState(meeting.title);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  const save = async (): Promise<void> => {
    const trimmed = editTitle.trim();
    setEditing(false);
    if (trimmed && trimmed !== meeting.title) {
      await renameMeeting(meeting.id, trimmed);
      onChanged();
    } else {
      setEditTitle(meeting.title);
    }
  };

  return (
    <div
      className="group relative w-full cursor-pointer rounded-xl border border-border bg-surface-1 p-4 text-left transition-colors hover:bg-surface-2"
      onClick={() => {
        if (!editing && !confirmDelete) void navigate({ to: '/meeting/$meetingId', params: { meetingId: meeting.id } });
      }}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0 flex-1">
          {editing ? (
            <input
              ref={inputRef}
              value={editTitle}
              onChange={(e) => setEditTitle(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void save();
                if (e.key === 'Escape') {
                  setEditTitle(meeting.title);
                  setEditing(false);
                }
              }}
              onClick={(e) => e.stopPropagation()}
              className="w-full border-b border-border bg-transparent pb-0.5 font-medium outline-none"
            />
          ) : (
            <h3 className="truncate font-medium">{meeting.title}</h3>
          )}
          <div className="mt-0.5 flex items-center gap-2 text-sm text-text-secondary">
            <span>{new Date(meeting.createdAt.replace(' ', 'T') + 'Z').toLocaleDateString()}</span>
            {meeting.tags.map((tag) => (
              <Badge key={tag} variant="neutral" className="px-1.5 py-0 text-[10px]">
                {tag}
              </Badge>
            ))}
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          {meeting.status === 'transcribing' && (
            <Badge variant="warning">
              transcribing…
            </Badge>
          )}
          {!editing && <Badge variant="neutral">{formatDuration(meeting.durationSeconds)}</Badge>}

          <div
            className={`flex gap-0.5 transition-opacity ${editing || confirmDelete ? '' : 'opacity-0 group-hover:opacity-100'}`}
            onClick={(e) => e.stopPropagation()}
          >
            {editing ? (
              <>
                <Button variant="ghost" size="icon-sm" onClick={() => { setEditTitle(meeting.title); setEditing(false); }}>
                  <X />
                </Button>
                <Button variant="ghost" size="icon-sm" className="text-mint hover:text-mint" onClick={() => void save()}>
                  <Check />
                </Button>
              </>
            ) : confirmDelete ? (
              <>
                <span className="mr-1 self-center text-xs text-text-secondary">Delete?</span>
                <Button variant="ghost" size="icon-sm" onClick={() => setConfirmDelete(false)}>
                  <X />
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  className="text-record-red hover:text-record-red"
                  onClick={() => void deleteMeeting(meeting.id).then(onChanged)}
                >
                  <Check />
                </Button>
              </>
            ) : (
              <>
                <Button variant="ghost" size="icon-sm" onClick={() => { setEditing(true); setEditTitle(meeting.title); }}>
                  <Pencil />
                </Button>
                <Button variant="ghost" size="icon-sm" onClick={() => setConfirmDelete(true)}>
                  <Trash2 />
                </Button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function SearchResults({ results }: { results: SearchResult[] }): React.ReactElement {
  if (results.length === 0) {
    return <p className="py-8 text-center text-sm text-text-tertiary">No matches.</p>;
  }
  return (
    <div className="flex flex-col gap-1">
      {results.map((r, i) => (
        <Link
          key={`${r.meetingId}-${r.source}-${i}`}
          to="/meeting/$meetingId"
          params={{ meetingId: r.meetingId }}
          className="rounded-lg px-3 py-2 transition-colors hover:bg-surface-1"
        >
          <div className="flex items-center gap-2">
            <span className="truncate text-sm font-medium">{r.title}</span>
            <Badge variant="neutral" className="px-1.5 py-0 text-[10px]">
              {r.source}
            </Badge>
          </div>
          {r.snippet && r.source !== 'title' && (
            <p className="mt-0.5 truncate text-xs text-text-secondary">{r.snippet}</p>
          )}
        </Link>
      ))}
    </div>
  );
}

function MeetingsPage(): React.ReactElement {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [query, setQuery] = useState('');
  const [debounced, setDebounced] = useState('');
  const [tagFilter, setTagFilter] = useState<string | null>(null);

  useEffect(() => {
    const t = setTimeout(() => setDebounced(query.trim()), 250);
    return () => clearTimeout(t);
  }, [query]);

  const { data: meetings, refetch } = useQuery({ queryKey: ['meetings'], queryFn: listMeetings });
  const { data: results } = useQuery({
    queryKey: ['search', debounced],
    queryFn: () => searchMeetings(debounced),
    enabled: debounced.length > 1,
  });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onMeetingReady(() => void refetch()).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [refetch]);

  const onChanged = (): void => {
    void queryClient.invalidateQueries({ queryKey: ['meetings'] });
  };

  const allTags = [...new Set((meetings ?? []).flatMap((m) => m.tags))].sort();
  const visible = (meetings ?? []).filter((m) => !tagFilter || m.tags.includes(tagFilter));
  const searching = debounced.length > 1;

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-6">
      <header className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold tracking-tight">Meetings</h1>
        <Button onClick={() => void navigate({ to: '/record' })}>
          <Mic /> New recording
        </Button>
      </header>

      <div className="relative">
        <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-text-tertiary" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search transcripts, notes, summaries…"
          className="h-10 w-full rounded-lg border border-border bg-surface-1 pl-9 pr-3 text-sm outline-none transition-colors placeholder:text-text-tertiary focus:border-mint/50"
        />
      </div>

      {allTags.length > 0 && !searching && (
        <div className="flex flex-wrap gap-1.5">
          {allTags.map((tag) => (
            <button
              key={tag}
              type="button"
              onClick={() => setTagFilter(tagFilter === tag ? null : tag)}
              className={`rounded-full border px-2.5 py-0.5 text-xs transition-colors ${
                tagFilter === tag
                  ? 'border-mint/50 bg-mint-subtle text-mint'
                  : 'border-border text-text-secondary hover:bg-surface-1'
              }`}
            >
              {tag}
            </button>
          ))}
        </div>
      )}

      {searching ? (
        <SearchResults results={results ?? []} />
      ) : !meetings || visible.length === 0 ? (
        <div className="py-16 text-center text-text-secondary">
          <p className="text-lg">No meetings yet</p>
          <p className="mt-1 text-sm">Start your first recording to see it here.</p>
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          {visible.map((m) => (
            <MeetingCard key={m.id} meeting={m} onChanged={onChanged} />
          ))}
        </div>
      )}
    </div>
  );
}
