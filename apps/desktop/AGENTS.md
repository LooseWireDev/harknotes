# apps/desktop — Desktop (Tauri v2)

Tauri v2 wraps the **Web** app (`apps/web/`) as a native desktop binary. This app has minimal source of its own — most of the desktop logic lives in `src-tauri/` (Rust) and the embedded `apps/web/` build.

## Stack

- **Tauri v2** — native shell
- **Rust** — backend (in `src-tauri/`)
- **Webview** — loads the built `apps/web` bundle

## Layout

```
apps/desktop/
├── package.json           # tauri scripts
├── src/                   # any desktop-only TS bridge code
└── src-tauri/             # Rust crate: tauri.conf.json, capabilities, main.rs
```

## Run / Build

The desktop app requires `apps/web` to be present and built.

```sh
pnpm --filter @harknotes/desktop dev     # tauri dev (loads web dev server)
pnpm --filter @harknotes/desktop build   # tauri build (bundles web + native)
```

## Conventions

- **Don't duplicate UI logic in `src/`.** Anything user-facing lives in `apps/web/`.
- **Native capabilities** are declared in `src-tauri/capabilities/`. Add a new permission there before invoking a Tauri command from the web side.
- **Rust code** in `src-tauri/` follows standard Tauri patterns. Don't introduce new dependencies without a clear reason.
