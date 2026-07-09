import { createRootRoute, Outlet } from '@tanstack/react-router';

export const Route = createRootRoute({
  component: RootLayout,
});

function RootLayout(): React.ReactElement {
  return (
    <main>
      <Outlet />
    </main>
  );
}
