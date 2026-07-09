import { createFileRoute } from '@tanstack/react-router';
import { invoke } from '@tauri-apps/api/core';
import { useQuery } from '@tanstack/react-query';

export const Route = createFileRoute('/')({
  component: HomePage,
});

function HomePage(): React.ReactElement {
  const { data, error } = useQuery({
    queryKey: ['ping'],
    queryFn: () => invoke<string>('ping'),
    retry: false,
  });

  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-2">
      <h1 className="text-2xl font-semibold">Harknotes</h1>
      <p className="text-sm text-neutral-500">
        {error ? 'Native bridge unavailable (running in a browser?)' : (data ?? 'Connecting…')}
      </p>
    </div>
  );
}
