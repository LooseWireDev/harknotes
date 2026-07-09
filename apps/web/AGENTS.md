# apps/web — Web App (React + Vite + TanStack Router)

## Stack

- **React 19** with **Vite**
- **TanStack Router** — file-based routing
- **TanStack Query** — server-state caching
- **tRPC client** — typesafe RPC against `apps/api` (Hono backend)
- **Better Auth** — auth client (paired with the Hono API's Better Auth server)
- **Tailwind CSS v4** + shadcn/ui primitives


## Layout

```
apps/web/
├── index.html
├── src/
│   ├── main.tsx           # entry: createRouter + QueryClient + tRPC
│   ├── routes/            # TanStack file-based routes
│   ├── components/        # shared UI
│   ├── hooks/             # custom React hooks
│   ├── features/          # feature-specific code (created by `feature` generator)
│   └── lib/               # utilities
│       ├── trpc.ts        # tRPC client
│       └── auth.ts        # Better Auth client
├── vite.config.ts
└── package.json
```

## Run / Test / Build

```sh
pnpm --filter @harknotes/web dev       # vite dev server
pnpm --filter @harknotes/web build     # tsc -b && vite build
pnpm --filter @harknotes/web test      # vitest run
pnpm --filter @harknotes/web preview   # serve built output
```

## Conventions

- **Routes are file-based** under `src/routes/`. Use TanStack Router's file conventions (`__root.tsx`, `index.tsx`, `$param.tsx`).
- **Components**: PascalCase, default export, no `any` props. Co-locate small components with the route that owns them; share larger ones via `src/components/`.
- **Server state** lives in TanStack Query.
  - Feed it via the tRPC client (`trpc.x.useQuery()`) — do not call `fetch` directly.
- **Auth state** lives in the Better Auth client hook. Do not roll your own auth context.

- **No barrel files** inside `src/` — import directly from the file that owns the export.

## Adding a new feature

```sh
pnpm nx generate @harknotes/project-plugin:feature --name <name> --apps web
```

