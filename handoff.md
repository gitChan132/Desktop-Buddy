# Handoff — desktop-buddy (Feeling Frog)

Read this first, then continue from "Next up".

## What this is
Tauri v2 + React desktop frog companion (`hackumass26/`). Watches typing/window telemetry, fires `trigger-intervention` events → speech bubble above frog. LLM copy via Gemini (`src-tauri/src/ai_engine.rs`), `GEMINI_API_KEY` from env/`.env`; deterministic fallback copy if LLM fails.

## Current state (Nov 14, 2026)
- LLM path working end-to-end (real HTTP, JSON parse, fallback). Bubble renders LLM copy, auto-dismisses at 10s (`INTERVENTION_TTL_MS`).
- Dev hooks in `src-tauri/src/ai_engine.rs`:
  - `FROG_FORCE_TRIGGER=<trigger>` env → fires one intervention 2s after launch.
  - Debug `eprintln!` tick log + `[ai_engine] emitted trigger=...` log — **temporary, remove before ship**.
- Global 5-min intervention cooldown per process (`INTERVENTION_COOLDOWN`) → one fire per app run; restart app between e2e tests.
- Eval tick every 20s; telemetry windows 30s. `FOCUS_APPS` list (window_tracker.rs) excludes Terminal — idle_freeze only counts browsers/editors.
- Tab thrash = >8 window switches in 20s (`THRASH_SWITCHES`), polled every 2s.

## Trigger test status
| Trigger | Status |
|---|---|
| frantic_typing | ✅ e2e fired (`keys>=180`; 250 injected keys) |
| backspace_loop | ✅ e2e fired (`backspaces>=24` streak 8) |
| tab_thrashing | ⚠️ detection counts switches (saw 8, threshold >8) but no fire yet. Attempted 16 Safari/Terminal activations @2.1s → only 8 counted (2s poll desyncs). Retry with 30 switches @1s. |
| idle_freeze | ❌ untested. Needs 6 min zero input with Safari/Chrome frontmost, then wait a tick + LLM. |

Copy path for all 4 tested via forced trigger — all produce bubble.

## Next up
1. Rerun tab_thrashing test with faster switching (`/tmp/inject_thrash.py` already rewritten to 30 @1s — last run was rejected; user needs to allow).
2. Run idle_freeze test: launch app, `open -a Safari`, no input 6.5 min, grep log for `emitted trigger=idle_freeze`.
3. Remove debug eprintln lines + `fire_forced`/`FROG_FORCE_TRIGGER` hook from ai_engine.rs when done.
4. Kill stray dev processes after tests (`pkill -f "tauri dev"`, `desktop-buddy`, `vite`).

## Test harness
- `/tmp/run_trigger_test.sh <trigger_name> <inject_script>` — restarts app, waits for launch, runs injector, polls log for `emitted trigger=`, screenshots on fire.
- Injectors (pyobjc/Quartz or osascript): `/tmp/inject_typing.py`, `/tmp/inject_backspace.py`, `/tmp/inject_thrash.py`.
- Logs: `/tmp/frog-test-<name>.log`.

## Run dev
```
cd /Users/punyagalundia/hackumass26 && npm run tauri dev
# one forced intervention:
FROG_FORCE_TRIGGER=backspace_loop npm run tauri dev
```
`cargo check` in `src-tauri/` must stay clean (known harmless warning: `block v0.1.6` future-incompat).
