# Testing checklist

Human-readable list of everything that exists right now. Tick things off as you
try them. If something fails, note what you did + what happened.

## 0. Before you start

- [ ] `npm run tauri dev` starts cleanly (port 1420 free, no stray `for-frogs-sake`).
- [ ] On macOS, the app/terminal has **Accessibility** permission (needed for triggers + selection).
- [ ] `GEMINI_API_KEY` is in the environment or `.env.local` (otherwise you get fallback copy).
- [ ] Rust changes need a dev restart; frontend changes hot-reload.

## 1. The frog itself

- [ ] Frog appears bottom-right, feet on the taskbar/menu-bar edge, idle animation playing.
- [ ] Click the frog with no nudge up → opens frog chat.
- [ ] Drag the frog → he dangles while held; release → he falls/stays where dropped.
- [ ] Fling the frog → throw physics, bounces off screen edges, lands on the desktop.
- [ ] While carried/thrown, clicks outside the frog don't fall through the window.
- [ ] Clicking elsewhere on the transparent window does nothing (click-through).

## 2. Nudges (speech bubble)

- [ ] A trigger shows a bubble above the frog with a message + activity label.
- [ ] Bubble has a thin timer bar that drains left→right.
- [ ] Bubble auto-dismisses after ~10s and the timer resets for the next nudge.
- [ ] Firing another nudge while one is visible swaps it cleanly (no overlap/jitter).
- [ ] Nudge copy comes from Gemini (different wording each time), not the same canned line.
- [ ] With no API key/network, the fallback line still appears and is readable (no `â€”` garbage).

## 3. Triggers

Fire each one and confirm the frog reacts with a nudge. Easiest path: use the
**Testing** tab (section 9) or the env hook `FROG_FORCE_TRIGGER=<id>`.

- [ ] `frantic_typing` — type very fast (≥180 keys / 30s).
- [ ] `backspace_loop` — hammer backspace (≥36 / 30s or a streak of 8).
- [ ] `cursor_hesitation` — click 3+ times, each after ~2s of stillness.
- [ ] `cursor_oscillation` — sweep the cursor left/right quickly (≥14 reversals / 10s).
- [ ] `tab_thrashing` — switch tabs repeatedly (>5 / 20s).
- [ ] `idle_freeze` — no input for 150s with browser/editor focused.
- [ ] `doomscroll` — feed site frontmost 4+ min, scroll a lot, little typing.
- [ ] YouTube Shorts variant — 30s on Shorts with ≥6 swipes/scrolls.
- [ ] Muted trigger (toggle in Settings) does not fire.
- [ ] Paused nudges (tray or Settings) stops everything.

## 4. Activity variety (recent change)

- [ ] Every trigger can eventually offer **any** of the four guides — not just its top pick.
- [ ] Two nudges in a row never offer the same activity (it rotates down the aptness list).
- [ ] Ranked order feels sensible (e.g. doomscroll prefers "Look away", typing prefers "Belly breathe").
- [ ] Turning an activity off in Settings removes it from the rotation; if only one is left, that one is used.
- [ ] If every suitable activity is muted, the nudge still opens the trigger's fallback guide.

## 5. Guided activities

Open via the nudge (click frog while a nudge is up) or the pond's Activities tab.

- [ ] **Belly breathe** — breathing animation, inhale/exhale pacing, frog croaks along.
- [ ] **Look away / visual grounding** — ~29s step-by-step guide.
- [ ] **One thing (task intention)** — name one priority + 20s hold/countdown.
- [ ] **Mindful pause** — 30s countdown then closes.
- [ ] Closing/completing a guide returns the frog to idle.
- [ ] Sound toggle affects croak sounds; volume slider changes loudness.
- [ ] Starting an activity while a nudge is up works; nudge doesn't block the guide.

## 6. Frog chat

- [ ] Click frog with no nudge → chat panel opens.
- [ ] Frog answers in persona, short replies.
- [ ] History is per-open; closing and reopening starts fresh.
- [ ] Guardrails: asking if it's an AI/model, prompt-injection attempts, homework, code, math, trivia → fixed deflecting line.
- [ ] Replies never mention AI/model or show code fences.
- [ ] Crisis/self-harm message → crisis guidance (988) is shown.
- [ ] Message over 400 chars is rejected/trimmed.

## 7. Pond panel

Open from the tray menu → **Control panel** → big pond window. Tabs:

- [ ] **Status** — Anxiety + Focus meters, live stats (keys/backspaces/etc.), sparkline history.
- [ ] **Settings** — see section 8.
- [ ] **Activities** — list of guides with descriptions, can start one.
- [ ] **Modes** — Focus / Chill / Fun with descriptions; current mode highlighted.
- [ ] **Testing** — see section 9.
- [ ] Frog hops onto the pond lilypad when it opens; drops back down when it closes.
- [ ] Moving the pond window doesn't break the hop/drop.
- [ ] Esc / close button closes the pond.
- [ ] Settings changed in the pond apply immediately (and tray checks stay in sync).

## 8. Settings

Session-only — defaults return after relaunch. Check each actually does something:

- [ ] Pause nudges.
- [ ] Croak sounds.
- [ ] Volume.
- [ ] Selection summaries.
- [ ] Per-trigger toggles (7).
- [ ] Per-activity toggles (4).
- [ ] Mode selection; settings panel shows when a value is overridden by a mode.

## 9. Testing tab + trigger remote (new)

Two entry points: pond **Testing** tab, and tray **Test triggers…** remote.

- [ ] Each trigger button fires that detector's nudge using synthetic input.
- [ ] Muted triggers are disabled and labelled "(muted)".
- [ ] "Random trigger" picks one enabled trigger.
- [ ] Each press uses the LLM → different message each time (no repeats).
- [ ] Demo nudge uses the normal 10s timer bar and closes by itself.
- [ ] Firing another trigger while the bubble is up swaps cleanly.
- [ ] Demo nudges also rotate activities (no back-to-back repeats).
- [ ] Pond closes when you fire from the Testing tab so you can see the frog.
- [ ] Remote sits next to the frog, stays on top, has its own Close button.
- [ ] Demo does not skip the cooldown for real nudges afterwards (cooldown untouched).

## 10. Selection summaries

- [ ] Highlight 400+ chars of text in VS Code / a browser and hold it ~2 ticks.
- [ ] Frog offers a summary.
- [ ] Accepting sends the passage to Gemini and shows the summary.
- [ ] Short selections do nothing.
- [ ] Same passage isn't offered again for 120s; different offers are 10s apart.
- [ ] Turning off selection summaries in Settings stops the watcher.
- [ ] Changing the selection quickly doesn't trigger a stale offer.

## 11. Tray menu

- [ ] Tray icon appears (macOS menu bar / Windows system tray).
- [ ] Left-click toggles the small control panel near the icon.
- [ ] Menu: Control panel, Pause nudges (check state), Croak sounds (check state), Test triggers…, Quit.
- [ ] Check marks match the pond settings after changing either one.
- [ ] Quit fully exits (no leftover process).

## 12. Modes & cooldowns

- [ ] Focus = 25s cooldown between real nudges.
- [ ] Fun = 30s.
- [ ] Chill = 120s cooldown, and doomscroll is muted regardless of its toggle.
- [ ] Leaving Chill restores the doomscroll toggle's previous state.

## 13. Platforms / edge cases

- [ ] Windows build compiles (macOS-only accessibility code is gated).
- [ ] Windows: frog sits on the taskbar, no taskbar entry, always-on-top works.
- [ ] macOS: frog stays above other apps' full-screen Spaces.
- [ ] Unfocused window: timers keep running (background throttling flags set).
- [ ] Multi-monitor: frog/pond/remote stay on the monitor they belong to.

## 14. Dev hooks (optional / destructive-ish, use deliberately)

- [ ] `FROG_FORCE_TRIGGER=<id>` fires one intervention ~2s after launch.
- [ ] `FROG_EVAL_SECS=<n>` speeds up the evaluation loop.
- [ ] `FROG_IDLE_FREEZE_SECS=<n>`, `FROG_DOOMSCROLL_SECS/SCROLLS`, `FROG_SHORTS_SECS/SCROLLS` override thresholds.
- [ ] `GEMINI_MODEL=<name>` switches the model (default `gemini-2.5-flash`).
- [ ] Debug logs (`[ai_engine] emitted trigger=...`, tick log) show expected values.

## 15. Build health

- [ ] `npx tsc --noEmit` clean.
- [ ] `npx vite build` clean.
- [ ] `cargo check` clean (only the known `block v0.1.6` third-party note).
- [ ] `cargo test` — all 55 pass.
