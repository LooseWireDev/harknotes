import { createRootRoute, Link, Outlet } from '@tanstack/react-router';

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout(): React.ReactElement {
  return (
    <div className="flex min-h-screen flex-col">
      <nav className="flex items-center gap-4 border-b border-neutral-200 px-6 py-3">
        <span className="font-semibold">Harknotes</span>
        <Link
          to="/"
          className="text-sm text-neutral-500 hover:text-neutral-900 [&.active]:font-medium [&.active]:text-neutral-900"
        >
          Meetings
        </Link>
        <Link
          to="/record"
          className="text-sm text-neutral-500 hover:text-neutral-900 [&.active]:font-medium [&.active]:text-neutral-900"
        >
          Record
        </Link>
      </nav>
      <main className="flex-1">
        <Outlet />
      </main>
    </div>
  );
}
