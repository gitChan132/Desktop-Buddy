# Instructions

At the start of every session, read `handoff.md` first — it holds current project state,
tested triggers, dev hooks, and next tasks. `scope.md` has the product spec and detection
matrix; `README.md` covers setup.

## Quick commands
- Run app: `npm run tauri dev` (needs macOS Accessibility permission + `GEMINI_API_KEY`).
- Rust tests: `cd src-tauri && cargo test --lib`.
- Frontend: `npx tsc --noEmit`, `npm run build`.

Keep `cargo check` clean in `src-tauri/` (one known harmless warning: `block v0.1.6`).
Kill stray dev processes when done: `pkill -f "tauri dev"; pkill -f desktop-buddy; pkill -f vite`.
