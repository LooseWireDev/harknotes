---
name: feature-development
description: The required workflow for adding a feature to this monorepo — spec, generator scaffold, failing tests, then implementation. Use whenever adding a new feature, endpoint, screen, or capability. Never create feature directories by hand.
---

# Feature Development

Every feature follows the same four-step flow. Do not skip steps or reorder them.

## 1. Spec first

A feature starts from a written spec (usually in `docs/`) covering behavior, edge cases, and acceptance criteria. If no spec exists, stop and ask for one — do not improvise requirements.

## 2. Scaffold with the generator

```sh
npx nx generate @<project>/project-plugin:feature --name <name> --apps <apps>
```

- `<project>` is this project's package scope (check the root `package.json` name or `tools/project-plugin/package.json`).
- `--name` must be a **camelCase identifier** starting with a lowercase letter: `userProfile`, not `user-profile` or `UserProfile`. It becomes `${name}Router` and directory names; the generator rejects anything else.
- `--apps` is a comma-separated subset of `api,web,mobile,desktop,static`.

What the generator creates per app:

| App | Files |
|---|---|
| api | `apps/api/src/features/<name>/` — `router.ts`, `service.ts`, `service.test.ts`, `types.ts` |
| web | `apps/web/src/features/<name>/` — `<Name>List.tsx`, `use<Name>Actions.ts`, `<Name>List.test.tsx` |
| mobile | `apps/mobile/src/features/<name>/` — same shape as web, React Native |
| desktop | `apps/desktop/src-tauri/src/commands/<name>.rs` |
| static | `apps/static/src/pages/<name>/index.astro` + island component |

Plus, always: a shared Zod schema stub at `packages/shared/src/schemas/<name>.ts` and an `AGENTS.md` stub in each feature directory.

**Wiring (Hono backends):** the generator automatically registers the feature router in `apps/api/src/trpc.ts` via the `// forge:feature-imports` and `// forge:feature-routers` anchor comments. Do not remove those anchors, and do not wire routers by hand.

**Python backends:** the generator scaffolds client-side features only. Create the `apps/api` feature module manually following `apps/api/AGENTS.md` and the fastapi-sqlalchemy-patterns skill.

## 3. Failing tests (separate session/agent from implementation)

Write tests from the spec into the generated `*.test.ts(x)` files — never from reading implementation. Run them; they must fail for the right reason (missing behavior, not syntax errors).

## 4. Implement

Minimum code to make the tests pass, in the generated files. Business logic goes in `service.ts`, not in the router. Fill in the feature's `AGENTS.md` stub (summary, business rules, schemas, relations) once behavior settles.

## Verify

`npx nx affected -t lint test typecheck` must pass before the feature is done.
