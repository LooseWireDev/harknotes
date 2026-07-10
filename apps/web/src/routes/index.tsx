import { createFileRoute, Link, useNavigate } from '@tanstack/react-router';
import { useQuery } from '@tanstack/react-query';

import { listMeetings, onMeetingReady, type Meeting } from '../lib/transcription';
import { useEffect } from 'react';

export const Route = createFileRoute('/')({
  component: MeetingsPage,
});

function statusBadge(status: Meeting['status']): React.ReactElement | null {
  switch (status) {
    case 'recording':
      return <span className="rounded-full bg-red-100 px-2 py-0.5 text-xs text-red-700">recording</span>;
    case 'transcribing':
      return (
        <span className="rounded-full bg-amber-100 px-2 py-0.5 text-xs text-amber-700">
          transcribing…
        </span>
      );
    default:
      return null;
  }
}

function formatDuration(totalSeconds: number): string {
  const m = Math.floor(totalSeconds / 60);
  const s = totalSeconds % 60;
  return `${m}:${String(s).padStart(2, '0')}`;
}

function MeetingsPage(): React.ReactElement {
  const navigate = useNavigate();
  const { data: meetings, refetch } = useQuery({
    queryKey: ['meetings'],
    queryFn: listMeetings,
  });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onMeetingReady(() => void refetch()).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [refetch]);

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-8">
      <header className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Meetings</h1>
        <button
          type="button"
          onClick={() => void navigate({ to: '/record' })}
          className="rounded-lg bg-emerald-600 px-4 py-2 font-medium text-white hover:bg-emerald-700"
        >
          New recording
        </button>
      </header>

      {!meetings || meetings.length === 0 ? (
        <p className="py-12 text-center text-neutral-400">
          No meetings yet — start your first recording.
        </p>
      ) : (
        <ul className="flex flex-col divide-y divide-neutral-100">
          {meetings.map((m) => (
            <li key={m.id}>
              <Link
                to="/meeting/$meetingId"
                params={{ meetingId: m.id }}
                className="flex items-center gap-3 rounded-md px-3 py-3 hover:bg-neutral-50"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium">{m.title}</p>
                  <p className="text-sm text-neutral-400">
                    {m.createdAt} · {formatDuration(m.durationSeconds)}
                  </p>
                </div>
                {statusBadge(m.status)}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
