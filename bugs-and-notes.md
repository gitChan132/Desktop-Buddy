# Bugs, gotchas and things to remember

Companion to `handoff-frontend.md`. This is the "don't step on this again" file:
known open issues first, then fixed bugs that are easy to reintroduce, then the
operational quirks of running and testing this thing.

Last updated: 2026-11-14 · branch `main`

---

## 1. Open issues

| # | Issue | Impact | Where | Notes / next step |
|---|---|---|---|---|
| 1 | **Precision-touchpad scrolling never reaches the wheel hook.** `scrolls30`/`scrolls90` stay `0` on the ThinkPad even while the user scrolls. | Instagram and TikTok doomscroll cannot trigger on that device. YouTube Shorts still works (flips). | `telemetry.rs` (`EventType::Wheel`) | rdev uses `WH_MOUSE_LL`; driver-emulated panning is delivered without `WM_MOUSEWHEEL`. Options: Raw Input (`WM_INPUT`), or a different signal for those feeds (e.g. pointer movement, UIA scroll). A physical mouse wheel does increment the counters. |
| 2 | **Intervention bubble can clip at screen edges.** The window is allowed to hang off-screen so the frog can reach the edges, so a bubble centred in the window gets cut. | Cosmetic, worst when resting hard left/right. | `FeelingFrog.tsx` bubble container | Shift the bubble horizontally based on `outerPosition` vs monitor width. |
| 3 | **45 s cooldown + 10 s evaluation = frequent nudges.** | Feels naggy during normal work; fine for demos. | `ai_engine.rs` `INTERVENTION_COOLDOWN` / `EVALUATION_INTERVAL` | Raise the cooldown when demoing is done. |
| 4 | **Shorts fast lane can fire from clicking, not just scrolling.** It counts tab-title changes, so clicking through recommended videos also counts as a flip. | False positive if someone is browsing YouTube by clicks. | `ai_engine.rs` `is_short_form_scroll` | Tighten by requiring regular flip intervals (e.g. no gap > 4 s) instead of a raw count. |
| 5 | **`site_key` is a guess.** Browsers expose no URL, so thrashing groups tabs by title heuristics: known-site list, else the last `" - "` segment. | Switching between two Google properties counts as one site; en-dash titles fall back to the whole title. | `window_tracker.rs` | Only fixable with real URLs (UI Automation on the address bar, or a browser extension). |
| 6 | **Chat and LLM copy need `GEMINI_API_KEY`.** Without it the app still works but uses canned replies. | Message quality only. | `chat.rs`, `ai_engine.rs` | `.env.local` is read automatically; model override via `GEMINI_MODEL`. |
| 7 | **Window position is not persisted.** Every launch snaps to the bottom-right work-area corner. | Cosmetic; surprising after dragging him somewhere. | `lib.rs` `position_bottom_right` | Not implemented. Adding `tauri-plugin-window-state` would restore the last position, but it would also override the startup placement, so decide which you want. |
| 8 | **Placing him at an edge can leave the window partly off-screen.** By design (sprite-aware bounds), and click/drag still work. | Confusing when inspecting the desktop. | `FeelingFrog.tsx` bounds | Not a bug; documented so nobody "fixes" it. |

---

## 2. Fixed bugs — do not reintroduce

1. **`event.currentTarget` is `null` after `await`.** The original drag called
   `setPointerCapture` after `await scaleFactor()`, which threw into a swallowed
   `try/catch`; capture never engaged, so fast cursor moves detached the frog and
   it got stuck in the held pose. **Always** stash `const button = event.currentTarget`
   before any await.
2. **Never follow pointer deltas for the window position.** Per-event
   `setPosition` lags under fast movement. A rAF loop polling `cursorPosition()`
   and setting `window = cursor − crown` is the working design.
3. **Release is not guaranteed by pointer capture alone.** The drag ends on
   `pointerup`/`pointercancel` **and** the Rust `pointer-release` event (global
   left-button release). Both paths must stay.
4. **Tailwind silently drops interpolated classes.** `` `absolute inset-0${cond ? " pixel-shake" : ""}` ``
   made the scanner lose `inset-0`, so the sprite span was 0×0 (frog invisible).
   Use static class strings per branch.
5. **The sprite sheet's rows are camera angles, not frames.** Each 4-column block
   has 8 rows of the same motion from other angles. Playing the whole block makes
   the frog spin. Only row 0 per animation.
6. **Collision bounds must use the sprite's visible box, not the window box.**
   Window-based bounds made him bounce ~150 px short of the screen edges.
   `SPRITE_BOX` + `FROG_BOTTOM_INSET` / `FROG_FEET_INSET_CSS` are the numbers.
7. **Startup placement must account for the feet inset (18 CSS px).** Otherwise he
   spawns floating a sprite-box height above the taskbar — the "hovering on spawn"
   bug.
8. **Chromium throttles timers in unfocused WebView2 windows.** The sprite died to
   ~1 fps until `tauri.conf.json` got
   `--disable-background-timer-throttling --disable-backgrounding-occluded-windows --disable-renderer-backgrounding`.
9. **Running the rdev listener on Tauri's main thread crashes Windows**
   (`0xc0000005`). Main thread on macOS (required by the tap), own thread elsewhere.
10. **Title changes are not window switches.** Counting them made Shorts flipping
    fire `tab_thrashing`. Now: different site/app → switch; same site, new title →
    content flip.
11. **A plain browser tab pointed at `localhost:1420` floods the Vite log with
    `getCurrentWindow ... metadata undefined` and `listen ... transformCallback`
    errors.** Those are *not* app errors — the page has no `__TAURI_INTERNALS__`.
    Close stray tabs before reading logs.
12. **The dev app dies (`0xffffffff`) when launched from a transient tool shell.**
    Start it detached (see §4).
13. **Windows: every window must declare the same `additionalBrowserArgs`.** When
    only the main window had the custom args, the new `panel` window made WebView2
    fail at startup with `HRESULT(0x8007139F)` / "Class not registered" and the app
    exited with code 101. Adding the identical args to `panel` in
    `tauri.conf.json` fixes it.
14. **Windows: `alwaysOnTop` is no longer in `tauri.conf.json`** (tao pins the
    window to a Space on macOS, breaking full-screen overlays). Without a
    replacement, other windows cover the frog on Windows, so `lib.rs` now calls
    `set_always_on_top(true)` for non-macOS in setup. Do not remove it.
15. **Every new window must be listed in `capabilities/default.json`.** The pond
    window was added without updating `"windows"`, so its `outerPosition`,
    `scaleFactor` and `listen` calls were denied ("event.listen not allowed on
    window \"pond\""). The catch blocks hid the failure: the pond computed a jump
    target of `(365,-121)` from a `(0,0)` origin, so the frog overshot the pad
    and the copy only appeared after the 3-second fallback. Capability windows
    are now `["main", "panel", "pond"]`.
16. **A one-shot `cancelled` flag in the frog's jump listener broke every jump
    after the first pond close.** The `pond-hidden` handler set `cancelled =
    true` for the lifetime of the effect, so after closing the pond once no
    later `pond-jump` ever moved the window again: the pond copy appeared via
    the 3-second fallback while the real frog stayed put. The listener now uses
    a run counter that each request bumps, `pond-hidden` only aborts the run in
    flight, and `start_pond_jump` stores the request (with a sequence number)
    so `FeelingFrog` can also poll `pending_pond_jump` every 200 ms — events
    are not replayed to a webview that was idle when they were emitted.

---

## 3. Numbers worth remembering

| Thing | Value |
|---|---|
| Window | 340×420 CSS (425×525 physical at 125%), frameless, transparent, always-on-top |
| Sprite | 512² sheet, 16×16 cells of 32 px, rendered at 192 px (`RENDER_SIZE`) |
| Art box in a cell | flight `x10..21 y11..26`, idle `x7..24 y20..30`, seated shock `x7..24 y19..30` |
| Eyes (seated shock 0) | sclera `x11..13` / `x18..20`, `y21..23`; pupils animated in DOM |
| Frogs feet above window bottom | 18 CSS px (`FROG_FEET_INSET_CSS`) |
| Crown grip | 15.5/32 × 13/32 of the 192 px button (≈ `209, 367` physical at 125%) |
| Breath cycle | 3 s inhale / 3 s hold / 5 s exhale (`breathing.ts`) |
| Doomscroll general | 4 min on feed + 40 scrolls/90 s + <10 keys/30 s |
| Doomscroll Shorts | 30 s on a YouTube window + 6 flips **or** 6 scrolls/30 s |
| Evaluation loop | every 10 s (`FROG_EVAL_SECS`) |
| Intervention cooldown | 45 s, shared by all triggers |
| Intervention bubble TTL | 10 s (`INTERVENTION_TTL_MS`) |
| Idle freeze | 150 s (`FROG_IDLE_FREEZE_SECS`) |
| Throw | 110 ms velocity window, max 4200 px/s, gravity 2600, drag e^−1.1t, bounce 0.55/0.45 |
| Tab thrash | >5 site switches in 20 s |

---

## 4. Running & testing

```powershell
# launch detached (transient shells kill the app)
cmd /c start "DeskBuddy dev" "C:\Users\Alex\AppData\Local\Temp\opencode\dev.cmd"

# or the env override variants used for detector tests
#   doom-dev.cmd  -> FROG_FORCE_TRIGGER=doomscroll
#   doom-dev2.cmd -> FROG_DOOMSCROLL_SECS=8 FROG_DOOMSCROLL_SCROLLS=8 FROG_EVAL_SECS=4
```

Checks:

```powershell
npx tsc --noEmit
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

### Env overrides (all read at process start)

| Var | Effect |
|---|---|
| `FROG_EVAL_SECS` | evaluation cadence (default 10) |
| `FROG_DOOMSCROLL_SECS` / `FROG_DOOMSCROLL_SCROLLS` | general doomscroll gate |
| `FROG_SHORTS_SECS` / `FROG_SHORTS_SCROLLS` | Shorts fast lane |
| `FROG_IDLE_FREEZE_SECS` | idle-freeze threshold |
| `FROG_FORCE_TRIGGER` | emit one intervention 2 s after launch with that trigger |
| `FROG_COOLDOWN_SECS` | **disabled** — the override is commented out in `ai_engine.rs`; restart to reset the cooldown |
| `GEMINI_API_KEY` / `GEMINI_MODEL` | LLM access |

### Reading the tick log

```
[ai_engine] tick keys=0 backspaces=0 streak=0 switches=2 thrash=false idle=false
  hesit=0 rev=1 feed=true(41s) flips=7 scrolls30=0 scrolls90=0 stress=0.21
```

- `feed=true(Ns)` — foreground was a feed for N s continuously.
- `flips` — title changes inside the same site (last 30 s). Drives Shorts.
- `scrolls30` / `scrolls90` — wheel ticks (0 on touchpad-only machines).
- `emitted trigger=...` lines confirm what actually fired.

### Test harness quirks

- Screenshots need DPI awareness (`SetProcessDpiAwarenessContext(-4)`); the display
  is 1920×1080 at 125%, so physical = CSS × 1.25.
- Synthetic clicks only reach the window once `setIgnoreCursorEvents(false)` has
  run — park the cursor over the frog ~900 ms before pressing.
- `SetForegroundWindow` is often denied; a real click on the target window is the
  reliable way to focus it. Watch out for terminals stealing foreground.
- WPF/shell `$LASTEXITCODE` after `Select-String` pipelines is unreliable; trust the
  printed `test result:` lines.
- Git writes progress to stderr, so PowerShell "errors" on `git push`/`pull` are
  usually noise; check the ref lines.
- `package-lock.json` likes to show platform-pruning churn; it was deliberately
  not committed. Revert it unless you mean to change dependencies.

---

## 5. Platform notes

- **Windows (this dev box):** everything above applies. The frog window is
  transparent/frameless; the taskbar is topmost, so the window intentionally hangs
  below the work area by ~23 px at 125% DPI.
- **macOS (teammate's machine):** rdev needs Accessibility permission; the
  window raises itself above full-screen Spaces (`lib.rs`). If wheel counting
  fails there too, check that permission first.
- The app is single-monitor happy: bounds use the current monitor's work area.
  Dragging to a different monitor and throwing may use stale bounds until a
  rethrow.
