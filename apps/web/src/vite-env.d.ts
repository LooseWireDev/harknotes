/// <reference types="vite/client" />

// TanStack Router auto-generates src/routeTree.gen.ts via the CLI plugin
// on `vite dev` / `vite build`. Declare it ambiently so tsc doesn't error
// before the first dev run produces the file.
declare module '*/routeTree.gen' {
  import type { AnyRoute } from '@tanstack/react-router';
  export const routeTree: AnyRoute;
}
