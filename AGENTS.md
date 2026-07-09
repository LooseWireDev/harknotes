# Agent Workflow & Project Conventions

You are a software engineer working in this monorepo. Follow the rules below. For app-specific conventions, read the per-app `AGENTS.md` files — they are hoisted into your context alongside this one.

## Per-app context

- `apps/api/AGENTS.md` — backend conventions (Hono/TypeScript or FastAPI/Python, depending on the project's framework choice)
- `apps/web/AGENTS.md` — web app conventions (if present)
- `apps/mobile/AGENTS.md` — mobile app conventions (if present)
- `apps/desktop/AGENTS.md` — desktop app conventions (if present)
- `apps/static/AGENTS.md` — static site conventions (if present)

Each app has its own `AGENTS.md` describing its layout, run/test commands, and framework-specific rules.

---

## Spec-Driven Development

- Always ask for a spec before implementing a feature.
- Session 1: Write failing tests from the spec. Do not implement.
- Session 2: Implement the minimum code to make tests pass.
- If you see implementation before tests exist, stop and ask for the spec.

## Feature Development

- Always use `nx generate @<project>/project-plugin:feature --name <name> --apps <apps>` before writing feature code.
- Feature names are camelCase identifiers (`userProfile`, not `user-profile`).
- Never create feature directories manually.
- The feature generator creates the correct file structure, imports, and wiring — including registering the tRPC router in `apps/api/src/trpc.ts` (Hono backends).
- Python backends: the generator scaffolds client-side features only; create `apps/api` feature modules manually following `apps/api/AGENTS.md`.

## Task Execution

- Use `nx affected` to run only what changed.
- Use `nx run <app>:<task>` for specific apps.
- Use `nx run-many --target=<task>` when you need to run across all apps.

---

## Language & Runtime

The backend (`apps/api/`) is either TypeScript (Hono + tRPC + Drizzle) or Python (FastAPI + SQLAlchemy + Alembic). The choice is made at scaffold time and recorded in the framework that wired up the project. Other apps are TypeScript. **Read `apps/api/AGENTS.md` to know which backend you're working in before writing any backend code.**

Node runtime for TS apps. Not Bun, not Deno. All TypeScript code is ESM. No CommonJS.

## File Naming

- camelCase for all files: `userService.ts`, `createPost.ts`, `authMiddleware.ts`
- React components use PascalCase: `UserProfile.tsx`, `DashboardLayout.tsx`
- Python files use snake_case: `user_service.py`, `auth_middleware.py`
- Test files sit next to what they test: `userService.test.ts` alongside `userService.ts`, `test_user_service.py` for Python
- No `index.ts` barrel files inside `apps/` directories. Only at package boundaries (`packages/ui/index.ts`, `packages/shared/index.ts`).

## Exports

- Default exports for React components only.
- Named exports for everything else: services, utilities, hooks, types, constants.
- Every exported function has an explicit return type annotation. No exceptions.

## Functions

- `function` keyword for all top-level and exported functions (TypeScript).
- Arrow functions for callbacks, inline handlers, and short expressions only.

## Error Handling

- TypeScript services throw `TRPCError` with the appropriate code. Python routers raise `HTTPException`.
- No try/catch blocks in business logic. Ever. The only try/catch blocks are at entry points: cron job handlers, background task runners, top-level process error handlers.

## Validation

- TypeScript: Zod everywhere. Every external input is validated. tRPC procedure inputs use Zod schemas.
- Python: Pydantic everywhere. Every external input is validated. FastAPI uses Pydantic request/response models.
- Shared schemas live in `packages/shared/` (TS) or `src/app/schemas/` (Py). Feature-specific schemas live in the feature's `types.ts` or `schemas/`.

## Testing Rules

- **NEVER** write tests after implementation in the same session.
- Always write failing tests first, based on the spec/requirements, before touching implementation.
- Tests must derive from expected behavior described in task requirements, not from reading the code.
- TypeScript: Vitest for unit/integration tests, fast-check for property-based tests on pure functions and data transformations.
- Python: pytest + pytest-asyncio + httpx for async integration tests. Test database via dependency override (see `apps/api/AGENTS.md`).
- No mocking unless the dependency is external (network calls, third-party APIs).
- Test names describe behavior: "returns 404 when user does not exist" not "calls db.findFirst and throws TRPCError".

## Linting and Formatting

- TypeScript: Biome handles both linting and formatting. No ESLint. No Prettier. Single quotes. Semicolons: yes. Tab width: 2 spaces. Trailing commas: all. Print width: 100.
- Python: ruff handles both linting and formatting. mypy for type checking. See `apps/api/AGENTS.md` for the exact configuration.

## Git

- Conventional commits: `feat:`, `fix:`, `chore:`, `docs:`, `test:`, `refactor:`
- One logical change per commit.

## Comments

- No obvious comments. Comment WHY, never WHAT.
- TODO comments include a brief reason: `// TODO: handle pagination once we have >100 users`

## Code Review Checklist

- Explicit return types on all exported functions
- External inputs validated (Zod in TS, Pydantic in Python)
- No try/catch in business logic
- No `any` types in TypeScript; equivalent care in Python
- No barrel files in app directories
- No CommonJS imports
- No mocking of internal dependencies in tests
- Conventional commit message

## What Not To Do

- Do not create `index.ts` barrel files inside `apps/` directories.
- Do not use `any` type. Use `unknown` and narrow.
- Do not use `require()` or CommonJS syntax.
- Do not mock internal dependencies in tests. Only mock external network calls.
- Do not write tests that mirror implementation. Tests come from specs.
- Do not use try/catch in service/business logic files.
- Do not write Python `print()` debug statements in API code — use the logger.
- Do not commit `.env` files or secrets.
