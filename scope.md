# The Feeling Frog 🐸 — Scope

A cozy, empathetic desktop companion that notices invisible cognitive friction
(executive dysfunction, ADHD paralysis, burnout) and offers a gentle micro-intervention
instead of more noise. Tauri v2 + React overlay living on a translucent lily pad in the
bottom-right of the screen.

## Product surface (implemented)

1. **Privacy-first implicit telemetry** — a Rust daemon reacts to global input timing and
   foreground-window changes. No key contents, text, clipboard, or screenshots are read.
   Buffers are bounded ring buffers (30s unless noted). Requires macOS Accessibility
   permission.
2. **Context-aware AI interventions** — when a signal crosses threshold the engine asks
   Gemini (`gemini-2.5-flash`) for one warm, frog-flavoured sentence that names the exact
   signal (e.g. "40 backspaces?"). Deterministic fallback copy is used if the model or key is
   unavailable. Each nudge fires once per 45 seconds per process.
3. **Kinetic overlay** — transparent, frameless, always-on-top window with a click-through
   hitbox. Spring-physics frog, draggable/carryable, glassmorphic speech bubble that
   auto-dismisses after 10s, and a guided breathing modal with a Web Audio chime.

## Detection matrix

| Trigger | What is monitored | Interpretation | Suggested action |
| --- | --- | --- | --- |
| `frantic_typing` | ≥180 keystrokes / 30s | Erratic typing cadence | `belly_breathe` |
| `backspace_loop` | ≥36 backspaces / 30s **or** streak ≥8 | Perfectionism / frustration spiral | `belly_breathe` |
| `cursor_hesitation` | ≥3 clicks each preceded by ≥2s of cursor stillness, / 30s | Decision difficulty | `belly_breathe` |
| `cursor_oscillation` | ≥6 significant direction reversals / 10s | Can't decide where to act next | `belly_breathe` |
| `tab_thrashing` | >8 foreground window switches / 20s | Fragmented focus | `visual_grounding` |
| `idle_freeze` | ≥150s (2.5 min) zero input while a browser/editor is frontmost | Stuck, not resting | `visual_grounding` |
| `doomscroll` | TikTok/Instagram app or a browser feed tab frontmost ≥4 min, ≥40 wheel events / 90s, <10 keystrokes / 30s | Compulsive scrolling | `visual_grounding` |

Priority order when several are true: backspace_loop → frantic_typing → cursor_hesitation →
cursor_oscillation → tab_thrashing → idle_freeze.

### Pond-ripple stress score (0.0–1.0)

Recomputed every 5s, contributes the payload `severity` (`low` <40, `medium` <70, `high` ≥70):

| Component | Saturates at | Weight |
| --- | --- | --- |
| Typing pressure | 180 keys / 30s | 0.25 |
| Backspace pressure | 36 backspaces / 30s | 0.15 |
| Pointer velocity | mean 3000 px/s | 0.15 |
| Pointer jitter | 2000 px/s std dev | 0.10 |
| Hesitation | mean 3s stillness before click | 0.15 |
| Oscillation | 8 reversals / 10s | 0.20 |

## Tech stack

- **Shell/UI:** Tauri v2 (Rust + React 19 / Vite / TypeScript), Tailwind CSS v4,
  Framer Motion (spring ramps, drag, AnimatePresence).
- **Telemetry (Rust):** `rdev` (global input timing), `active-win-pos-rs` (foreground app),
  `reqwest` (Gemini HTTP).
- **Audio:** native Web Audio API synth — no bundled audio assets.

## Architecture / file map

### Backend (`src-tauri/src/`)

- `lib.rs` — builder, startup wiring, bottom-right window positioning.
- `telemetry.rs` — global input listener + stress ticker; keypress/backspace buffers,
  pointer speed & jitter, click hesitations, horizontal direction reversals; emits
  `cursor-pos`, `pond-stress`, `pointer-release`.
- `window_tracker.rs` — 2s foreground-window poll, 20s switch history, tab-thrash + idle
  freeze detection; builds the combined `WindowSnapshot`.
- `ai_engine.rs` — 20s evaluation loop, intervention selection, Gemini request/JSON parse,
  fallback copy, emits `trigger-intervention`.

### Frontend (`src/`)

- `App.tsx` — 120ms click-through hitbox tick (`setIgnoreCursorEvents`), event listener,
  10s auto-dismiss, mood routing.
- `components/FeelingFrog.tsx` — sprite rig, hop/carry physics, speech bubble.
- `components/BreatheAlong.tsx` — guided breathing modal + chime.
- `components/VisualGrounding.tsx` — ~29s 20-20-20 guide: look away, find three
  things, notice each one's colour/shape/texture.
- `breathing.ts` — shared 3s inhale / 3s hold / 5s exhale rhythm.
- `types.ts` — shared IPC contract (`BurnoutTrigger`, `FrogInterventionPayload`, TTL).

## Shared IPC contract (`src/types.ts`)

```typescript
export type BurnoutTrigger =
  | 'frantic_typing' | 'backspace_loop' | 'tab_thrashing'
  | 'idle_freeze' | 'cursor_hesitation' | 'cursor_oscillation'
  | 'doomscroll';

export interface FrogInterventionPayload {
  id: string;
  trigger: BurnoutTrigger;
  severity: 'low' | 'medium' | 'high';
  message: string;
  suggestedAction: 'belly_breathe' | 'visual_grounding';
  activeApp?: string;
}
```

Emitted on the `trigger-intervention` Tauri event; bubbles hold for
`INTERVENTION_TTL_MS` (10s).

## Status

- All seven triggers verified end-to-end against the live app (real injection + Gemini).
- 40 Rust unit tests passing; `tsc --noEmit` and `vite build` clean.
- Fast-test env overrides: `FROG_EVAL_SECS`, `FROG_IDLE_FREEZE_SECS`,
  `FROG_FORCE_TRIGGER` (see `handoff.md`).
- Pre-ship cleanup: remove debug `eprintln!` tick/emit logs and the `FROG_FORCE_TRIGGER` hook.
- macOS-first (Accessibility + transparent-window private API enabled).
