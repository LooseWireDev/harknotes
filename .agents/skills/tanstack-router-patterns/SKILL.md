---
name: tanstack-router-patterns
description: Canonical patterns for the web app's TanStack Router + TanStack Query setup — file-based routing, the generated route tree, data fetching, and API client wiring. Use when adding routes, pages, or data fetching in apps/web.
---

# TanStack Router Patterns (apps/web)

## File-based routing

Routes live in `apps/web/src/routes/`. The route tree is **generated** into `src/routeTree.gen.ts` by the TanStack Router Vite plugin — never edit that file, and never import route components manually to "wire" them; creating the file in `src/routes/` IS the wiring.

- `__root.tsx` — root layout: `createRootRoute({ component: RootLayout })`, renders shared nav and `<Outlet />`.
- `index.tsx` — `/` via `createFileRoute('/')`.
- `about.tsx` → `/about`; `posts.$postId.tsx` → `/posts/:postId`; `_layout.tsx` files create pathless layouts.

```tsx
import { createFileRoute } from '@tanstack/react-router';

export const Route = createFileRoute('/about')({
  component: AboutPage,
});

function AboutPage(): React.ReactElement {
  return <div>…</div>;
}
```

Type safety comes from the `declare module '@tanstack/react-router' { interface Register { router: typeof router } }` block in `main.tsx` — leave it in place. Use `<Link to="/...">` (typed) instead of raw anchors, and `Route.useParams()` / `Route.useSearch()` for route data.

## Pages vs features

Route files stay thin: layout + composition. Real UI and logic live in `src/features/<name>/` (scaffolded by the feature generator — see the feature-development skill) and shared pieces in `src/components/`. A route file imports the feature's components; it does not contain business logic.

## Data fetching

`main.tsx` already provides a `QueryClientProvider`.

- **Hono backend:** use the typed tRPC hooks from `src/lib/trpc.ts` — `trpc.<feature>.<procedure>.useQuery()` / `.useMutation()`. When adding the FIRST tRPC call, wrap the app in `<trpc.Provider client={createTRPCClient()} queryClient={queryClient}>` in `main.tsx` (the scaffold leaves a comment marking the spot). Types flow from `@<project>/shared/types` — never define API response types by hand.
- **Python backend:** use `src/lib/apiClient.ts` (reads `VITE_API_URL`) with TanStack Query (`useQuery({ queryKey, queryFn })`), types from `src/lib/api-schema.ts` (regenerate with `pnpm run codegen`).

Prefer route `loader`s for data a page cannot render without; use component-level queries for everything else.

## Auth (Hono backend)

`src/lib/auth.ts` exposes the Better Auth client (`useSession`, `signIn`, `signOut`). Gate protected routes with a `beforeLoad` redirect on the route definition, not by conditionally rendering inside components.

## Conventions that trip people up here

- Components are the only default exports; hooks/utils are named exports with explicit return types.
- No barrel `index.ts` files anywhere under `apps/web/src/`.
- Tests (`*.test.tsx`) sit next to what they test and assert behavior, not markup structure.
