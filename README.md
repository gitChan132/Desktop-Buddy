# The Feeling Frog 🐸

A cozy desktop companion that watches for invisible signs of burnout — backspace spirals,
frantic typing, tab-thrashing, decision paralysis, idle freezes — and hops in with one warm,
frog-flavoured micro-intervention. Built with **Tauri v2 + React + Rust** for HackUMass.

The frog lives on a translucent lily pad in the bottom-right of your screen, is draggable,
and only intercepts the cursor inside its own hitbox. Interventions appear as a
glassmorphic speech bubble and auto-dismiss after 10 seconds. Clicking him opens a
persona-locked chat, and the tray/top-bar pond panel holds status, settings, guided
activities and modes.

## Detection

| Trigger | Signal |
| --- | --- |
| `frantic_typing` | ≥180 keystrokes in 30s |
| `backspace_loop` | ≥24 backspaces in 30s, or a streak of 6 |
| `cursor_hesitation` | ≥3 clicks each after ≥2s of cursor stillness (30s) |
| `cursor_oscillation` | ≥6 back-and-forth cursor reversals in 10s |
| `tab_thrashing` | >8 foreground window switches in 20s |
| `idle_freeze` | 2.5 min of zero input while a browser/editor is frontmost |
| `doomscroll` | feed frontmost ≥4 min with ≥40 scrolls / 90s (30s fast lane for Shorts) |

Each intervention's copy comes from Gemini (`gemini-2.5-flash` by default) with a
deterministic fallback. Left to itself the app sends only timing metadata and the active
window title — no key contents, screenshots or clipboard. Nudge spacing depends on the
mode: Focus 25s, Chill 120s, Fun 30s (default Focus).

**The one exception is selection summaries.** When you highlight a long passage (400+
chars) the frog offers to summarize it; if you accept, that passage is sent to Gemini.
The watcher reads the selection through macOS Accessibility only, never the clipboard,
and the offer can be switched off in the pond's settings.

## Beyond nudges

- **Guided activities** — belly breathing, look-away grounding, one-thing intention,
  and a 30s mindful pause. Each is its own component; the frog joins on the lilypad.
- **Frog chat** — click him when no nudge is up. Fixed persona, no AI talk, no history
  kept between opens.
- **Pond panel** (tray/top bar) — status sparklines, settings (sounds, volume, pause
  nudges, selection summaries, per-trigger and per-activity toggles), activities, and
  modes (Focus / Chill / Fun).
- **Modes** — Focus nudges early and judges everything; Chill stretches the cooldown to
  2 min and mutes the doomscroll detector; Fun keeps it brisk and light.

See [`scope.md`](scope.md) for the full product scope and [`handoff.md`](handoff.md) for
current state and dev notes.

## Requirements

- macOS (transparent-window private API + global input hooks)
- Node 18+ and Rust stable
- **Accessibility permission** granted to the app/terminal that launches it
- A Gemini API key

## Run

```bash
npm install
# put GEMINI_API_KEY=... in .env.local (or export it)
npm run tauri dev
```

Fire a single intervention immediately without waiting on telemetry:

```bash
FROG_FORCE_TRIGGER=backspace_loop npm run tauri dev
```

### Test-only overrides

| Env var | Effect |
| --- | --- |
| `FROG_FORCE_TRIGGER=<trigger>` | Emit one intervention 2s after launch |
| `FROG_EVAL_SECS=<n>` | Evaluation-loop cadence (default 20s) |
| `FROG_IDLE_FREEZE_SECS=<n>` | Idle-freeze threshold (default 150s) |
| `GEMINI_MODEL=<id>` | Override the Gemini model |

## Tests

```bash
cd src-tauri && cargo test --lib   # Rust unit tests (no GUI, no waits)
npx tsc --noEmit                   # frontend typecheck
npm run build                      # production frontend build
```

## Stack

Tauri v2 · React 19 · TypeScript · Vite · Tailwind CSS v4 · Framer Motion ·
Rust (`rdev`, `active-win-pos-rs`, `reqwest`, `objc2` + ApplicationServices AX) ·
Web Audio API.
