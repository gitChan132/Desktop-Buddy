import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { motion } from "framer-motion";
import { useCallback, useEffect, useRef, useState } from "react";
import pondArt from "../assets/frog/pond1.png";
import greenSpritesheet from "../assets/frog/green_spritesheet.png";
import { applySoundSettings, playCroak } from "../croak";
import {
  DEFAULT_SETTINGS,
  loadSettings,
  saveActivityEnabled,
  saveMode,
  saveNudgesPaused,
  saveSelectionSummaries,
  saveSoundEnabled,
  saveTriggerEnabled,
  saveVolume,
  watchSettings,
  type ActivityId,
  type FrogSettings,
  type PondMode,
  type TriggerId,
} from "../settings";

/** Panel window is 760x540 CSS px (see tauri.conf.json). */
const FROG_SIZE = 190;
/** Where he lands: top-left of the frog sprite, on the pond's lilypad. */
const LAND = { x: 440, y: 97 };

const SHEET_COLUMNS = 16;
const IDLE_FRAMES = 4;

/** Small wait helper for the entrance choreography. */
const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

type View = "status" | "settings" | "activities" | "modes";

interface PondStats {
  stress: number;
  focus: number;
  keys: number;
  backspaces: number;
  switches: number;
  hesitantClicks: number;
  reversals: number;
  scrolls30s: number;
  onFeed: boolean;
  feedSeconds: number;
  secondsSinceInput: number;
  activeApp: string | null;
  nudgesPaused: boolean;
  soundEnabled: boolean;
}

const TRIGGER_ROWS: { id: TriggerId; key: keyof FrogSettings["triggers"]; label: string }[] = [
  { id: "frantic_typing", key: "franticTyping", label: "Frantic typing" },
  { id: "backspace_loop", key: "backspaceLoop", label: "Backspace loop" },
  { id: "tab_thrashing", key: "tabThrashing", label: "Tab thrashing" },
  { id: "idle_freeze", key: "idleFreeze", label: "Idle freeze" },
  { id: "cursor_hesitation", key: "cursorHesitation", label: "Cursor hesitation" },
  { id: "cursor_oscillation", key: "cursorOscillation", label: "Cursor oscillation" },
  { id: "doomscroll", key: "doomscroll", label: "Doomscroll" },
];

const ACTIVITY_ROWS: {
  id: ActivityId;
  key: keyof FrogSettings["activities"];
  label: string;
}[] = [
  { id: "belly_breathe", key: "bellyBreathe", label: "Belly breathe" },
  { id: "visual_grounding", key: "visualGrounding", label: "Look away" },
  { id: "task_intent", key: "taskIntent", label: "One thing" },
  { id: "mindful_pause", key: "mindfulPause", label: "Mindful pause" },
];

const ACTIVITY_INFO: Record<
  ActivityId,
  { label: string; blurb: string; steps: { text: string; ms: number }[] }
> = {
  belly_breathe: {
    label: "Belly breathe",
    blurb: "3 in · 3 hold · 5 out",
    steps: [
      { text: "Breathe in through your nose, slow and even.", ms: 3000 },
      { text: "Hold it, soft. Nothing clenched.", ms: 3000 },
      { text: "And out, long and quiet. Rrrribbit.", ms: 5000 },
    ],
  },
  visual_grounding: {
    label: "Look away",
    blurb: "rest your eyes on the room",
    steps: [
      { text: "Look away from the screen. Let your eyes land far away.", ms: 6000 },
      { text: "Find three things you can see. Name them quietly.", ms: 7000 },
      { text: "Notice one colour and one shape.", ms: 7000 },
      { text: "Notice what the surface under you feels like.", ms: 7000 },
    ],
  },
  task_intent: {
    label: "One thing",
    blurb: "name it, hold it",
    steps: [
      { text: "Say the one thing that matters right now. Just one.", ms: 6000 },
      { text: "Hold it in your head. Nothing else needs doing for a moment.", ms: 20000 },
    ],
  },
  mindful_pause: {
    label: "Mindful pause",
    blurb: "hands off, thirty seconds",
    steps: [
      { text: "Hands off the keyboard. Let them rest.", ms: 8000 },
      { text: "Feel the next breath arrive on its own.", ms: 8000 },
      { text: "Nothing to fix. The pond is still here.", ms: 8000 },
    ],
  },
};

const MODES = [
  {
    id: "focus",
    label: "Focus",
    blurb: "Block feeds and nudge earlier.",
  },
  {
    id: "chill",
    label: "Chill",
    blurb: "Lets you scroll and do other stuff.",
  },
  {
    id: "fun",
    label: "Fun",
    blurb: "Jumps around your screen and does fun stuff.",
  },
] as const;

/** Loops the idle row, the jump row while airborne, or shows a fixed pose. */
function PondFrog({
  breatheFrame,
  pose,
  jumping,
}: {
  breatheFrame: number | null;
  pose: { column: number; row: number } | null;
  jumping: boolean;
}) {
  const [idleFrame, setIdleFrame] = useState(0);
  const [jumpFrame, setJumpFrame] = useState(0);

  useEffect(() => {
    const id = setInterval(
      () => setIdleFrame((current) => (current + 1) % IDLE_FRAMES),
      1000 / 6,
    );
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    if (!jumping) {
      setJumpFrame(0);
      return;
    }
    const id = setInterval(
      () => setJumpFrame((current) => (current + 1) % 4),
      1000 / 10,
    );
    return () => clearInterval(id);
  }, [jumping]);

  const column = jumping
    ? 8 + jumpFrame
    : pose
      ? pose.column
      : breatheFrame !== null
        ? 4 + breatheFrame
        : idleFrame;
  const row = jumping ? 4 : pose ? pose.row : 0;

  return (
    <span
      aria-hidden
      style={{
        display: "block",
        width: FROG_SIZE,
        height: FROG_SIZE,
        backgroundImage: `url(${greenSpritesheet})`,
        backgroundSize: `${SHEET_COLUMNS * FROG_SIZE}px ${SHEET_COLUMNS * FROG_SIZE}px`,
        backgroundPosition: `${-column * FROG_SIZE}px ${-row * FROG_SIZE}px`,
        backgroundRepeat: "no-repeat",
        imageRendering: "pixelated",
      }}
    />
  );
}

/** Frames 0..2 over a 3-3-5 breath so the frog puffs along. */
function useBreathFrame(active: boolean) {
  const [frame, setFrame] = useState(0);

  useEffect(() => {
    if (!active) {
      setFrame(0);
      return;
    }
    const started = performance.now();
    const id = setInterval(() => {
      const t = ((performance.now() - started) / 1000) % 11;
      const next =
        t < 1 ? 0 : t < 2 ? 1 : t < 3 ? 2 : t < 6 ? 2 : t < 7.7 ? 2 : t < 9.4 ? 1 : 0;
      setFrame(next);
    }, 150);
    return () => clearInterval(id);
  }, [active]);

  return active ? frame : null;
}

function Sparkline({ values, color }: { values: number[]; color: string }) {
  return (
    <div className="pixel-meter flex h-[30px] items-end gap-[2px] overflow-hidden px-[3px] pb-[3px]">
      {values.map((value, index) => (
        <span
          key={index}
          className="w-[3px]"
          style={{
            height: `${Math.max(3, Math.round(value * 22))}px`,
            background: color,
          }}
        />
      ))}
    </div>
  );
}

function Meter({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div className="mt-1.5">
      <div className="flex items-baseline justify-between">
        <span className="text-[13px] uppercase tracking-[0.06em] text-emerald-100/90">
          {label}
        </span>
        <span className="text-[15px] leading-none text-emerald-50">
          {Math.round(value * 100)}%
        </span>
      </div>
      <div className="pixel-meter mt-0.5 h-[12px] w-full p-[2px]">
        <div
          className="h-full"
          style={{ width: `${Math.round(value * 100)}%`, background: color }}
        />
      </div>
    </div>
  );
}

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between border-b border-emerald-200/10 py-[2px]">
      <span className="text-[12px] uppercase tracking-[0.04em] text-emerald-100/60">
        {label}
      </span>
      <span className="text-[14px] leading-none text-emerald-50">{value}</span>
    </div>
  );
}

function Toggle({
  label,
  note,
  on,
  onChange,
}: {
  label: string;
  /** Shown under the label when the setting is overridden elsewhere. */
  note?: string;
  on: boolean;
  onChange: () => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      onClick={onChange}
      className="flex w-full items-center justify-between border-b border-emerald-200/10 py-[3px] text-left"
    >
      <span className="min-w-0 pr-2">
        <span className="block text-[13px] leading-none text-emerald-50/90">{label}</span>
        {note && (
          <span className="mt-0.5 block text-[10px] leading-none text-emerald-200/60">
            {note}
          </span>
        )}
      </span>
      <span
        aria-hidden
        className={`flex h-[14px] w-[26px] items-center border-2 border-[#04170f] px-[2px] ${
          on ? "justify-end bg-emerald-400" : "justify-start bg-emerald-950/60"
        }`}
      >
        <span className="h-[6px] w-[8px] bg-[#04170f]" />
      </span>
    </button>
  );
}

export function SettingsPond() {
  const [settings, setSettings] = useState<FrogSettings>(DEFAULT_SETTINGS);
  const [stats, setStats] = useState<PondStats | null>(null);
  const [history, setHistory] = useState<{ stress: number; focus: number }[]>([]);
  const [view, setView] = useState<View>("status");
  const [running, setRunning] = useState<{ id: ActivityId; step: number } | null>(null);
  const [turnRow, setTurnRow] = useState<number | null>(null);
  const [visible, setVisible] = useState(false);
  const runId = useRef(0);
  const lastDirection = useRef<"left" | "right">("right");
  const landedRef = useRef(false);

  // Settings: load, keep in sync, and mirror into the croak synth.
  useEffect(() => {
    let live = true;
    void loadSettings()
      .then((next) => {
        if (!live) return;
        setSettings(next);
        applySoundSettings(next);
      })
      .catch(() => {
        // Backend unavailable (browser preview): keep defaults.
      });
    const unlisten = watchSettings((next) => {
      setSettings(next);
      applySoundSettings(next);
    });
    return () => {
      live = false;
      void unlisten.then((off) => off());
    };
  }, []);

  // Stats: poll the backend and keep a short rolling history for the sparklines.
  useEffect(() => {
    let live = true;
    const tick = () => {
      void invoke<PondStats>("frog_stats")
        .then((next) => {
          if (!live) return;
          setStats(next);
          setHistory((previous) =>
            [...previous, { stress: next.stress, focus: next.focus }].slice(-40),
          );
        })
        .catch(() => {
          // Backend unavailable (browser preview).
        });
    };
    tick();
    const id = setInterval(tick, 2000);
    return () => {
      live = false;
      clearInterval(id);
    };
  }, []);

  // Landing: the real frog has arrived on the pad; take over, face away, then
  // turn back to the viewer.
  const finishLanding = useCallback(async () => {
    const run = runId.current;
    landedRef.current = true;
    setVisible(true);
    setTurnRow(4);
    playCroak();
    await sleep(280);
    if (run !== runId.current) return;

    const back = lastDirection.current === "right" ? [3, 2, 1, 0] : [5, 6, 7, 0];
    for (const row of back) {
      setTurnRow(row);
      await sleep(90);
      if (run !== runId.current) return;
    }
    setTurnRow(null);
  }, []);

  // Entrance: leave him on the desktop for a beat, then ask the real frog
  // window to turn and jump across the desktop onto the pond's lilypad.
  const runEntrance = useCallback(async () => {
    const run = ++runId.current;
    landedRef.current = false;
    setVisible(false);
    setTurnRow(null);
    setRunning(null);

    let pondOrigin = { x: 0, y: 0 };
    let scale = 1;
    let anchor: { x: number; y: number } | null = null;
    try {
      const win = getCurrentWindow();
      const [origin, factor] = await Promise.all([
        win.outerPosition(),
        win.scaleFactor(),
      ]);
      pondOrigin = origin;
      scale = factor;
      anchor = await invoke<{ x: number; y: number } | null>("frog_anchor");
    } catch {
      // Browser preview: no windows to coordinate.
    }

    // A full beat sitting exactly where he was on the desktop.
    await sleep(1000);
    if (run !== runId.current) return;

    // Main frog centre is (167, 366) in its 340x420 window; pond frog centre is
    // (LAND.x + 92, LAND.y + 148). Move the real window so those coincide.
    const targetX = pondOrigin.x + (LAND.x + 92 - 167) * scale;
    const targetY = pondOrigin.y + (LAND.y + 148 - 366) * scale;
    const direction =
      anchor && targetX + 167 * scale < anchor.x ? "left" : "right";
    lastDirection.current = direction;

    try {
      await invoke("start_pond_jump", {
        request: { targetX, targetY, direction },
      });
    } catch {
      // Browser preview: just stand him on the pad.
      setVisible(true);
      setTurnRow(0);
      return;
    }

    // If the overlay never reports back, show him on the pad anyway.
    window.setTimeout(() => {
      if (run === runId.current && !landedRef.current) void finishLanding();
    }, 3000);
  }, [finishLanding]);

  useEffect(() => {
    const unlisten = listen("pond-jump-done", () => {
      void finishLanding();
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, [finishLanding]);

  // The popover can ask for a specific tab, and every open bumps a show counter.
  // Poll a (tab, seq, showSeq) triple instead of trusting an event payload: the
  // pond's webview may still be waking up when the request fires.
  const lastTabSeq = useRef<number | null>(null);
  const lastShowSeq = useRef(0);
  useEffect(() => {
    let live = true;
    const poll = async () => {
      try {
        const requested = await invoke<{
          tab: string;
          seq: number;
          showSeq: number;
        }>("pond_tab");
        if (!live) return;
        if (lastTabSeq.current !== requested.seq) {
          lastTabSeq.current = requested.seq;
          if (
            requested.tab === "activities" ||
            requested.tab === "modes" ||
            requested.tab === "settings"
          ) {
            setView(requested.tab);
          } else {
            setView("status");
          }
        }
        if (lastShowSeq.current !== requested.showSeq) {
          lastShowSeq.current = requested.showSeq;
          if (requested.showSeq > 0) void runEntrance();
        }
      } catch {
        // Backend unavailable (browser preview).
      }
    };
    void poll();
    const id = setInterval(poll, 300);
    return () => {
      live = false;
      clearInterval(id);
    };
  }, [runEntrance]);

  const close = useCallback(() => {
    void invoke("hide_pond").catch(() => {
      void getCurrentWindow().hide();
    });
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [close]);

  // Activity runner: advance lines, loop breathing, stop everything else.
  useEffect(() => {
    if (!running) return;
    const info = ACTIVITY_INFO[running.id];
    const step = info.steps[running.step];
    if (!step) {
      setRunning(null);
      return;
    }
    const id = setTimeout(() => {
      setRunning((current) => {
        if (!current) return null;
        const next = current.step + 1;
        if (next < info.steps.length) return { id: current.id, step: next };
        return current.id === "belly_breathe" ? { id: current.id, step: 0 } : null;
      });
    }, step.ms);
    return () => clearTimeout(id);
  }, [running]);

  // Croak when a new activity line lands.
  useEffect(() => {
    if (running) playCroak();
  }, [running?.id, running?.step]);

  const breatheFrame = useBreathFrame(running?.id === "belly_breathe");

  const setPaused = () => {
    const nudgesPaused = !settings.nudgesPaused;
    setSettings((previous) => ({ ...previous, nudgesPaused }));
    void saveNudgesPaused(nudgesPaused).catch(() => {});
  };

  const setSound = () => {
    const soundEnabled = !settings.soundEnabled;
    setSettings((previous) => ({ ...previous, soundEnabled }));
    void saveSoundEnabled(soundEnabled).catch(() => {});
  };

  const setSummaries = () => {
    const selectionSummaries = !settings.selectionSummaries;
    setSettings((previous) => ({ ...previous, selectionSummaries }));
    void saveSelectionSummaries(selectionSummaries).catch(() => {});
  };

  const pickMode = (mode: PondMode) => {
    setSettings((previous) => ({ ...previous, mode }));
    void saveMode(mode).catch(() => {});
  };

  const changeVolume = (volume: number) => {
    setSettings((previous) => ({ ...previous, volume }));
    void saveVolume(volume).catch(() => {});
  };

  const toggleTrigger = (id: TriggerId, key: keyof FrogSettings["triggers"]) => {
    const enabled = !settings.triggers[key];
    setSettings((previous) => ({
      ...previous,
      triggers: { ...previous.triggers, [key]: enabled },
    }));
    void saveTriggerEnabled(id, enabled).catch(() => {});
  };

  const toggleActivity = (id: ActivityId, key: keyof FrogSettings["activities"]) => {
    const enabled = !settings.activities[key];
    setSettings((previous) => ({
      ...previous,
      activities: { ...previous.activities, [key]: enabled },
    }));
    void saveActivityEnabled(id, enabled).catch(() => {});
  };

  const stress = stats?.stress ?? 0;
  const focus = stats?.focus ?? 0;
  const stressHistory = history.map((sample) => sample.stress);
  const focusHistory = history.map((sample) => sample.focus);

  const runningInfo = running ? ACTIVITY_INFO[running.id] : null;
  const runningStep = runningInfo && running ? runningInfo.steps[running.step] : null;

  const headerTitle =
    view === "settings"
      ? "Settings"
      : view === "activities"
        ? "Activities"
        : view === "modes"
          ? "Modes"
          : "Pond status";

  return (
    <div
      className="relative h-full w-full overflow-hidden"
      style={{
        backgroundImage: `url(${pondArt})`,
        backgroundSize: "cover",
        backgroundPosition: "center",
        fontFamily: '"VT323", ui-monospace, monospace',
      }}
    >
      {/* Left column: status / settings / activities / modes */}
      <div className="pixel-hud absolute bottom-4 left-4 top-4 flex w-[236px] flex-col px-3 pb-2 pt-0">
        <div className="-mx-3 flex items-center justify-between bg-[#04170f] px-2 py-1.5">
          <span className="text-[15px] uppercase tracking-[0.1em] text-emerald-100">
            {headerTitle}
          </span>
          <div className="flex items-center gap-1.5">
            {view === "status" ? (
              <>
                <span
                  className={`text-[11px] uppercase tracking-[0.08em] ${
                    settings.nudgesPaused ? "text-amber-300" : "text-emerald-300/80"
                  }`}
                >
                  {settings.nudgesPaused ? "paused" : "live"}
                </span>
                <button
                  type="button"
                  aria-label="Settings"
                  onClick={() => setView("settings")}
                  className="pixel-gear px-1 text-[15px] leading-none text-emerald-100"
                >
                  ⚙
                </button>
              </>
            ) : (
              <button
                type="button"
                aria-label="Back to status"
                onClick={() => setView("status")}
                className="px-1 text-[15px] leading-none text-emerald-100"
              >
                ←
              </button>
            )}
          </div>
        </div>

        {view === "status" && (
          <>
            <Meter label="Anxiety" value={stress} color="#e0748c" />
            <Sparkline values={stressHistory} color="#e0748c" />

            <Meter label="Focus" value={focus} color="#5ccf72" />
            <Sparkline values={focusHistory} color="#5ccf72" />

            <div className="mt-2 flex-1">
              <StatRow label="keys / 30s" value={String(stats?.keys ?? 0)} />
              <StatRow label="backspaces" value={String(stats?.backspaces ?? 0)} />
              <StatRow label="site switches" value={String(stats?.switches ?? 0)} />
              <StatRow label="hesitation" value={String(stats?.hesitantClicks ?? 0)} />
              <StatRow label="cursor flips" value={String(stats?.reversals ?? 0)} />
              <StatRow label="scrolls / 30s" value={String(stats?.scrolls30s ?? 0)} />
              <StatRow
                label="on feed"
                value={stats?.onFeed ? `${stats.feedSeconds}s` : "no"}
              />
              <StatRow label="idle" value={`${stats?.secondsSinceInput ?? 0}s`} />
              <StatRow label="in front" value={stats?.activeApp ?? "-"} />
            </div>

            <div className="mt-1 flex gap-1.5">
              <button
                type="button"
                onClick={() => setView("activities")}
                className="pixel-button flex-1 px-1.5 py-1 text-[12px] uppercase leading-none text-emerald-900"
              >
                Activities
              </button>
              <button
                type="button"
                onClick={() => setView("modes")}
                className="pixel-button flex-1 px-1.5 py-1 text-[12px] uppercase leading-none text-emerald-900"
              >
                Modes
              </button>
            </div>
          </>
        )}

        {view === "settings" && (
          <div className="mt-1 flex-1 overflow-y-auto pr-1">
            <p className="text-[11px] uppercase tracking-[0.08em] text-emerald-200/60">
              General
            </p>
            <Toggle label="Croak sounds" on={settings.soundEnabled} onChange={setSound} />
            <Toggle label="Pause nudges" on={settings.nudgesPaused} onChange={setPaused} />
            <Toggle
              label="Summarize long selections"
              on={settings.selectionSummaries}
              onChange={setSummaries}
            />

            <div className="mt-1.5">
              <div className="flex items-baseline justify-between text-[13px] uppercase">
                <span>Volume</span>
                <span>{Math.round(settings.volume * 100)}%</span>
              </div>
              <input
                type="range"
                min={0}
                max={100}
                step={5}
                value={Math.round(settings.volume * 100)}
                disabled={!settings.soundEnabled}
                aria-label="Croak volume"
                onChange={(event) => changeVolume(Number(event.target.value) / 100)}
                className="pixel-range mt-1 w-full disabled:opacity-40"
              />
            </div>

            <p className="mt-2 text-[11px] uppercase tracking-[0.08em] text-emerald-200/60">
              Triggers
            </p>
            {TRIGGER_ROWS.map((row) => (
              <Toggle
                key={row.id}
                label={row.label}
                note={
                  row.id === "doomscroll" && settings.mode === "chill"
                    ? "muted in chill mode"
                    : undefined
                }
                on={settings.triggers[row.key]}
                onChange={() => toggleTrigger(row.id, row.key)}
              />
            ))}

            <p className="mt-2 text-[11px] uppercase tracking-[0.08em] text-emerald-200/60">
              Activities
            </p>
            {ACTIVITY_ROWS.map((row) => (
              <Toggle
                key={row.id}
                label={row.label}
                on={settings.activities[row.key]}
                onChange={() => toggleActivity(row.id, row.key)}
              />
            ))}
          </div>
        )}

        {view === "activities" && (
          <div className="mt-1 flex-1 overflow-y-auto pr-1">
            {running && runningInfo && runningStep ? (
              <>
                <p className="text-[15px] uppercase leading-none tracking-[0.06em] text-emerald-100">
                  {runningInfo.label}
                </p>
                <p className="mt-0.5 text-[12px] leading-[1.15] text-emerald-100/60">
                  {runningInfo.blurb}
                </p>
                <p className="mt-2 min-h-[42px] text-[15px] leading-[1.15] text-emerald-50">
                  {runningStep.text}
                </p>
                <div className="pixel-meter mt-1 h-[10px] w-full p-[2px]">
                  <motion.div
                    key={`${running.id}-${running.step}`}
                    initial={{ width: 0 }}
                    animate={{ width: "100%" }}
                    transition={{ duration: runningStep.ms / 1000, ease: "linear" }}
                    className="h-full bg-emerald-400"
                  />
                </div>
                <button
                  type="button"
                  onClick={() => setRunning(null)}
                  className="pixel-button mt-2 w-full px-2 py-1 text-[12px] uppercase leading-none text-emerald-900"
                >
                  Stop
                </button>
              </>
            ) : (
              <>
                <p className="text-[12px] leading-[1.15] text-emerald-100/60">
                  Pick something to do right now. The frog joins in on the lilypad.
                </p>
                {ACTIVITY_ROWS.map((row) => {
                  const info = ACTIVITY_INFO[row.id];
                  const enabled = settings.activities[row.key];
                  return (
                    <button
                      key={row.id}
                      type="button"
                      disabled={!enabled}
                      onClick={() => setRunning({ id: row.id, step: 0 })}
                      className="mt-1.5 w-full border-2 border-[#04170f] bg-emerald-800/60 px-2 py-1 text-left disabled:opacity-40"
                    >
                      <span className="block text-[14px] uppercase leading-none text-emerald-50">
                        {info.label}
                      </span>
                      <span className="mt-0.5 block text-[11px] leading-none text-emerald-100/60">
                        {enabled ? info.blurb : "switched off in settings"}
                      </span>
                    </button>
                  );
                })}
              </>
            )}
          </div>
        )}

        {view === "modes" && (
          <div className="mt-1 flex-1 overflow-y-auto pr-1">
            <p className="text-[12px] leading-[1.15] text-emerald-100/60">
              Sets how often the frog interrupts and what he'll judge you for.
            </p>
            {MODES.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => pickMode(item.id)}
                className={`mt-1.5 w-full border-2 border-[#04170f] px-2 py-1 text-left ${
                  settings.mode === item.id ? "bg-emerald-500/80" : "bg-emerald-800/60"
                }`}
              >
                <span className="block text-[15px] uppercase leading-none text-emerald-50">
                  {item.label}
                </span>
                <span className="mt-0.5 block text-[11px] leading-[1.1] text-emerald-100/70">
                  {item.blurb}
                </span>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* The real frog flies in from the desktop; this copy takes over on landing */}
      <motion.div
        className="pointer-events-none absolute left-0 top-0"
        initial={false}
        animate={{ x: LAND.x, y: LAND.y }}
        transition={{ duration: 0.08 }}
        style={{ opacity: visible ? 1 : 0 }}
      >
        <PondFrog
          breatheFrame={breatheFrame}
          pose={turnRow === null ? null : { column: 0, row: turnRow }}
          jumping={false}
        />
      </motion.div>

      {/* Pond label */}
      <div className="absolute right-[4%] top-[8%] text-right">
        <p className="text-[22px] uppercase leading-none tracking-[0.14em] text-emerald-50 drop-shadow-[2px_2px_0_rgba(4,23,15,0.8)]">
          The pond
        </p>
        <p className="text-[14px] text-emerald-100/80 drop-shadow-[1px_1px_0_rgba(4,23,15,0.8)]">
          how your pond is doing
        </p>
      </div>
    </div>
  );
}
