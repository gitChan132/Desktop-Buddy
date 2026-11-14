# Handoff — Frontend Track (Feeling Frog)

Last updated: 2026-11-14
Repo: `gitChan132/Desktop-Buddy` · branch `main`

This is a catch-up note for the React/Tauri-overlay side of The Feeling Frog. Read
this first, then `src/components/FeelingFrog.tsx` — almost all the character work
lives there.

---

## 1. Where things stand

At the time of writing:

- `origin/main` now includes "Add cursor hesitation and oscillation detection"; the rig
  work below is all committed and the working tree is clean.
- The backend detects **six** triggers — `frantic_typing`, `backspace_loop`,
  `tab_thrashing`, `idle_freeze`, plus the new `cursor_hesitation` and
  `cursor_oscillation` (see `scope.md` / `handoff.md`).
- `npx tsc --noEmit`, `npm run build`, and `cargo test --lib` (35 tests) all pass.
- The app runs and every behaviour below has been verified in the real Tauri window, not
  just in a browser tab.

Recent history (newest first):

```
e5353db Bounce the frog off the screen edges, not the window box
20ce4cd Keep the frog under the cursor while carrying him and always let go
17af21f Replace the breathing modal with a croak-voiced breathing guide
3c10b61 Add LLM-backed Feeling Frog interventions with detection detail   <- backend
b714fca Throw the frog: fling him with mouse velocity and a startled pose
fcc25f4 Pick the frog up by the forehead, dangling as you carry him
```

---

## 2. What the app is

A transparent, frameless, always-on-top 340×420 window parked near the
bottom-right of the desktop. Inside it is a pixel-art frog (the only visible
thing; the rest of the window is click-through) that:

- idles/breathes, croaks when an intervention lands,
- can be **dragged and thrown** with real physics,
- opens a **guided breathing session** when clicked,
- receives intervention payloads from the Rust engine over Tauri events.

### File map

| File | Role |
|---|---|
| `src/App.tsx` | Window shell, click-through hitbox loop, event listener, mood wiring, mounts frog + guide |
| `src/components/FeelingFrog.tsx` | Character rig: sprite animation, drag/carry, throw physics, intervention bubble, click-to-open |
| `src/components/BreatheAlong.tsx` | Guided breathing bubble above the frog (bar, lines, phase chip, croak sound) |
| `src/breathing.ts` | Shared breath phases (3s inhale / 3s hold / 5s exhale) used by both the guide and the sprite |
| `src/assets/frog/green_spritesheet.png` | The green sprite sheet (16×16 grid of 32px cells) |
| `src/assets/fonts/vt323.woff2` | Bundled retro dialogue font (VT323, OFL) — no network needed at runtime |
| `src/styles.css` | Base styles + `.pixel-panel` / `.pixel-chip` / `.pixel-button` / `.pixel-bar` / `.pixel-tail` |
| `src-tauri/src/telemetry.rs` | Emits `cursor-pos`, `pond-stress`, and `pointer-release` (see §5) |
| `src-tauri/tauri.conf.json` | Transparent window + `additionalBrowserArgs` (see §6) |
| `src-tauri/capabilities/default.json` | Window permissions (position, cursor, ignore-cursor-events) |

`LilyPadGrounding.tsx` was deleted — `BreatheAlong.tsx` replaced the old
full-screen breathing modal.

---

## 3. Recently landed frontend work

Now committed on `main`:

1. **Breathing bar now breathes instead of looping.**
   `BreatheAlong.tsx` sets `barFrom`/`barTo`: 0→100% on inhale, stays 100% on
   hold, 100→0% on exhale. The phase label (`3 · 3 · 5`) is now derived from
   `BREATH_PHASES` instead of hard-coded.

2. **Frog's cheeks follow the breath.**
   New `breathPhase` prop on `FeelingFrog` (from `App`, fed by
   `BreatheAlong`'s `onPhase`). While it is set the sprite uses the croak row:
   - inhale → frames `[0, 1, 2]` (throat puffs up, one third of the phase each)
   - hold → frame `[2]` (cheeks full)
   - exhale → `[2, 1, 0]` (lets the puff go)
   Held/flight poses still take precedence if you grab him mid-session.

3. **New shared module** `src/breathing.ts` (`BreathPhase`, `BREATH_PHASES`).

Verified in-app: 20% bar + small throat at ~1s, ~85% + full puff at ~5s, bar
solid and cheeks full during hold, bar draining and cheeks easing back during
exhale; closing returns him to the idle pose.

---

## 4. How the frog rig works

### Sprite sheet layout

512×512, 16×16 cells of 32px. Each animation owns a **4-column block whose first
row is the front-facing loop**; the 7 rows below are the same motion from other
angles/poses. Playing the whole block makes the frog rotate on the spot — only
row 0 is used.

| Band | Column | Frames | Used for |
|---|---|---|---|
| idle | 0 | 4 | idle 6fps |
| croak | 4 | 4 | curious 8fps, breathing pull (frames 0–2) |
| jump | 8 | 4 | held dangle (frames 1–2), flight flex (frame 2) |
| hop | 12 | 4 | defined, unused |
| shock/hurt | row 8 | 5 | flight wide-eyed pose (frame 1) |

Rendered at `RENDER_SIZE = 192` via `background-position` on a background sized
`3072px 3072px` (16 cells × 192px) with `imageRendering: "pixelated"`.

Art bounding boxes inside a cell (used for physics/hit geometry):
- flight frames (shock 1, jump 2): `x 10..21, y 11..26`
- idle: `x 7..24, y 20..30`

### Moods

`FrogMood = "idle" | "curious" | "croaking"`.
`App` picks: guide open → `croaking`, intervention → `curious`, else `idle`.
The intervention speech bubble is hidden while `croaking`.

### Drag & carry

- On `pointerdown`, the crown offset (15.5/32 × 13/32 of the button) is measured
  from the button's layout box. **Grab anywhere and he snaps to your cursor by
  the forehead.**
- A `requestAnimationFrame` loop polls `cursorPosition()` every frame and sets
  `window = cursor − crown`. Do **not** go back to per-event deltas: that is what
  made him detach on fast movement.
- Movement is detected from the same poll (4px threshold), so a single huge
  cursor jump still picks him up.
- `onCarry` tells `App` to force the window interactive so the click-through
  tick cannot cut input mid-drag.
- Pose while carried: jump frames 1–2, alternating at 2fps.

### Throw

- Release velocity comes from the last 110ms of polled cursor samples, clamped to
  4200 px/s.
- A rAF integrator applies gravity (2600 px/s²), air drag (exp 1.1/s), edge
  bounces (0.55 x, 0.45 y) and ground friction; settles below 40 px/s.
- **Collision bounds use the sprite's visible box, not the window box.** The
  window is much larger than the frog, so window-based bounds made him bounce
  ~150px short of the screen edges. `SPRITE_BOX` + `FROG_BOTTOM_INSET` in
  `FeelingFrog.tsx` convert the art box to window-relative insets.
- The window is therefore allowed to hang partly off-screen.
- Release with no recent motion still flies (from rest) so he always falls.

### Breathing guide (`BreatheAlong`)

- Two boxes above the frog: the speech bubble (title, animated line, bar, phase
  dots, round, Close) and a dark chip with the live cue and a tail pointing at
  his head.- Lines rotate per round (`LINES[phase][round === 1 ? 0 : 1]`).
- A short filtered-saw croak plays at each exhale start; audio is best-effort
  and silently disabled if Web Audio is blocked.
- The overlay is `pointer-events-none` except the boxes, so the frog remains
  visible and draggable.

### Pixel UI kit

Text boxes are styled through `styles.css` classes, not a pile of utilities:

- `.pixel-panel` — VT323, 3px ink border, zero radius, hard 5px offset shadow,
  faint 8px checkerboard dither, inner 2px highlight.
- `.pixel-chip` + `.pixel-tail` — dark HUD badge and the stepped tail that points
  at the frog.
- `.pixel-button` — square Close button; the shadow collapses on `:active` so it
  looks pressed.
- `.pixel-bar` — bordered bar with a 1px inner highlight; fill is a square
  `motion.div` whose width animates.

The font lives at `src/assets/fonts/vt323.woff2` (bundled by Vite), so the
overlay never needs the network. Change the palette in one place: the greens are
literal hex values in those classes and match the sprite (`#0b3a1f` ink,
`#e9fbe2` paper).

---

## 5. Wiring to the backend

Events listened to in `App.tsx`:

| Event | Payload | Use |
|---|---|---|
| `trigger-intervention` | `FrogInterventionPayload` | mood, speech bubble, hop |
| `pointer-release` | `()` | ends a carry when the mouse is released outside the window |

Rust emits `cursor-pos` and `pond-stress` too; the frontend does **not** consume
them yet (candidate for stress-reactive moods).

`FrogInterventionPayload` (`src/types.ts`): `id`, `trigger`
(`frantic_typing | backspace_loop | tab_thrashing | idle_freeze | cursor_hesitation | cursor_oscillation | doomscroll`), `severity`,
`message`, `suggestedAction`
(`belly_breathe | visual_grounding | task_intent | mindful_pause`), optional
`activeApp`. Each trigger has a ranked activity list and the backend picks one
at random per intervention. Clicking the frog opens the guide for its
`suggestedAction`: breathing, `VisualGrounding.tsx`, `TaskIntention.tsx`, or
`MindfulPause.tsx` (30s hands-off pause). With no nudge up, the click opens
`ChatWithFrog.tsx`, which calls the persona-locked `frog_chat` Tauri command.

### Doomscroll detector

- `telemetry.rs` counts wheel events (`EventType::Wheel`) in a 90-second rolling
  window; `scroll_snapshot()` exposes the count. No content or coordinates.
- `window_tracker.rs` flags the foreground window as a feed when it is a TikTok
  or Instagram app, or a browser tab whose title contains
  `instagram | youtube | tiktok | shorts | reels`. `feed_seconds` tracks how long
  that feed has been in front and resets the moment focus lands elsewhere.
  Scope is deliberately tight — Reddit/X/etc. do **not** count.
- `ai_engine.rs` fires `doomscroll` when all of: `on_feed`, `feed_seconds >= 240`,
  `scrolls_90s >= 40`, `keys_30s < 10`. It is checked after the other triggers.
  The LLM prompt gets a sterner clause for this trigger ("playfully stern and
  disappointed"); the deterministic fallback is written the same way.
- **Shorts fast lane:** on a YouTube window (title contains `youtube`/`shorts`)
  with short-form cadence — `feed_seconds >= 30` and either `scrolls_30s >= 6`
  or `content_flips >= 6` — the same trigger fires after half a minute instead
  of four. Flips matter because precision-touchpad scrolls often never reach the
  wheel hook, while every new short retitles the tab. The check runs before
  cursor-oscillation and tab-thrashing so those can't steal it.
- **Tab thrashing is site-based:** `site_key()` infers a website from the tab
  title (browser suffix stripped, known site names matched, else the last ` - `
  segment). Switching between YouTube tabs, or Instagram tabs, is a *content
  flip*, not a window switch; only moving between different sites counts.
- Evaluation runs every 10 s, so the Shorts rule lands roughly 30–40 s after the
  window takes focus. The 45-second intervention cooldown still applies.
- Demo/test overrides: `FROG_DOOMSCROLL_SECS`, `FROG_DOOMSCROLL_SCROLLS`,
  `FROG_SHORTS_SECS`, `FROG_SHORTS_SCROLLS` (alongside `FROG_EVAL_SECS`).
- Frontend: `App` maps `trigger === "doomscroll"` to the `grumpy` mood;
  `FeelingFrog` shows the **seated** wide-eyed shock frame (shock 0, open mouth)
  with `.pixel-shake`, plus a DOM overlay that repaints the eye whites and
  animates the pupils left/right (`.pixel-look`). The overlay sits inside the
  sprite span so it shakes with him; its coordinates assume that shock-0 cell.

---

## 6. Gotchas worth remembering

1. **`event.currentTarget` is null after an `await`.** This silently broke
   `setPointerCapture` for the whole first version of dragging. Capture the
   element into a local before awaiting.
2. **Chromium throttles timers in unfocused WebView2 windows.** The sprite barely
   animated until `tauri.conf.json` got:
   `--disable-background-timer-throttling --disable-backgrounding-occluded-windows --disable-renderer-backgrounding`
   (passed via `additionalBrowserArgs` together with the default WebView2 flags).
3. **Click-through hitbox** (`useClickThroughHitbox`) polls the cursor every
   120ms and flips `setIgnoreCursorEvents`. It must be force-disabled while the
   guide is open or while carrying, otherwise input dies mid-interaction.
4. **The dev app dies when launched from a transient shell** (`0xffffffff`).
   Launch it detached (see §7).
5. **Screenshotting needs DPI awareness** (`SetProcessDpiAwarenessContext(-4)`);
   display is 1920×1080 at 125%, window is 425×525 physical / 340×420 CSS.
6. Windows from `setPosition` may sit partly off-screen by design now; the
   click-through logic and drag still work.

---

## 7. Running it

```bash
# from the repo root (macOS; grant Accessibility to the launching terminal)
npm run tauri dev
```

- Vite on **port 1420** (`strictPort`); free it first if a stray dev server survives:
  `lsof -ti tcp:1420 | xargs kill -9`.
- Typecheck: `npx tsc --noEmit` · Rust tests: `cd src-tauri && cargo test --lib`.
- Frontend edits hot-reload; Rust/config edits rebuild and restart automatically.
- Kill strays when done: `pkill -f "tauri dev"; pkill -f desktop-buddy; pkill -f vite`.
- §6.4/§6.5 (transient-shell death, 125% DPI offsets) were Windows-era notes; this repo
  now runs on macOS.

LLM interventions need whatever `.env.local` / API key the backend expects; the
engine is expected to fall back to canned messages without one.

---

## 8. Known rough edges / good next steps

1. **Intervention bubble clipping at screen edges.** Because the window may hang
   off-screen for the frog to reach an edge, the bubble (centred in the window)
   can be cut off when he rests hard against the left/right edge. Fix would be to
   shift the bubble horizontally based on the window's screen position.
2. **Startup placement matches the throw landing.** `position_bottom_right()` in
   `src-tauri/src/lib.rs` drops the window by `FROG_FEET_INSET_CSS` (18 CSS px,
   the gap between the window edge and the frog's feet) so he spawns resting on
   the taskbar rather than floating a sprite-box height above it. The transparent
   window therefore hangs ~23px below the work area at 125% DPI, exactly as it
   does after a throw.
3. **`hop` sprite band is unused**; the intervention hop is still framer-motion
   (`hop.start({ y: [-26, -14, 0] … })`). Could be replaced with the sprite jump
   row.
4. **Four actions exist.** `task_intent` → `TaskIntention.tsx`, `visual_grounding`
   → `VisualGrounding.tsx`, `mindful_pause` → `MindfulPause.tsx`,
   `belly_breathe` → breathing. Triggers list several apt actions (backend picks
   randomly); future guides get their own action id and branch in
   `App.openGrounding`. No-nudge clicks branch to `chat`.
5. **Unused backend events** `cursor-pos` / `pond-stress` — wiring the stress
   score into `FrogMood` (e.g. faster croak, worried pose) is the obvious next
   feature.
6. **Breathing is tied to sprite frames in thirds**, because the croak row only
   has three puff levels. If more granularity is wanted, add frames to the sheet
   or interpolate with CSS transforms.

---

## 9. Verification recipes used so far

- **Sprite animation alive:** screenshot the frog band every ~120ms, compare MD5s.
- **Throw:** synthetic `mouse_event` drag, then sample `GetWindowRect` every
  100–200ms to see the post-release trail and settle point.
- **Grab:** drag to an arbitrary point, check `cursor − window.topLeft` equals the
  crown offset (≈ `209, 367` at 125% DPI).
- **Breathing guide:** open, capture at ~0.5s / ~2.5s / ~4s / ~7s to hit inhale,
  late inhale, hold, exhale.
