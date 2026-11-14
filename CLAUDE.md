# Instructions

At the start of every session, read `handoff.md` first — it holds current project state,
tested triggers, dev hooks, settings/modes, the selection watcher, and next tasks.
`scope.md` has the product spec and detection matrix; `README.md` covers setup;
`handoff-frontend.md` and `bugs-and-notes.md` cover the React side and the traps.

Settings live in `src-tauri/src/settings.rs` + `src/settings.ts` — add a field to both
sides together, plus a `set_*` command, and every window picks it up over `frog-settings`.

## Quick commands
- Run app: `npm run tauri dev` (needs macOS Accessibility permission + `GEMINI_API_KEY`).
- Rust tests: `cd src-tauri && cargo test --lib`.
- Frontend: `npx tsc --noEmit`, `npm run build`.

Keep `cargo check` clean in `src-tauri/` (one known harmless warning: `block v0.1.6`).
Kill stray dev processes when done: `pkill -f "tauri dev"; pkill -f desktop-buddy; pkill -f vite`.
