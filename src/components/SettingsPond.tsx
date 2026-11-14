import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { motion } from "framer-motion";
import { useCallback, useEffect, useState } from "react";
import lilypadPond from "../assets/frog/lilypad-pond.jpg";
import greenSpritesheet from "../assets/frog/green_spritesheet.png";
import {
  loadSettings,
  saveNudgesPaused,
  saveSoundEnabled,
  watchSettings,
  type FrogSettings,
} from "../settings";

/** Panel window is 760x540 CSS px (see tauri.conf.json). */
const PANEL_W = 760;
const PANEL_H = 540;
const FROG_SIZE = 190;
/** Where he lands: top-left of the frog sprite, on a clear lilypad. */
const LAND = { x: 293, y: 199 };

const SHEET_COLUMNS = 16;
const IDLE_FRAMES = 4;

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

const DEFAULT_SETTINGS: FrogSettings = {
  nudgesPaused: false,
  soundEnabled: true,
  volume: 0.5,
};

/** Loops the idle row of the sprite sheet at `size` px. */
function PondFrog() {
  const [frame, setFrame] = useState(0);

  useEffect(() => {
    const id = setInterval(
      () => setFrame((current) => (current + 1) % IDLE_FRAMES),
      1000 / 6,
    );
    return () => clearInterval(id);
  }, []);

  return (
    <span
      aria-hidden
      style={{
        display: "block",
        width: FROG_SIZE,
        height: FROG_SIZE,
        backgroundImage: `url(${greenSpritesheet})`,
        backgroundSize: `${SHEET_COLUMNS * FROG_SIZE}px ${SHEET_COLUMNS * FROG_SIZE}px`,
        backgroundPosition: `${-frame * FROG_SIZE}px 0px`,
        backgroundRepeat: "no-repeat",
        imageRendering: "pixelated",
      }}
    />
  );
}

/** A row of chunky pixel bars showing the last ~80 seconds of a score. */
function Sparkline({
  values,
  color,
}: {
  values: number[];
  color: string;
}) {
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

/**
 * Pond-themed settings window. Opened from the tray ("Pond settings"); the frog
 * hops in from wherever he was sitting on the desktop, then idles on a lilypad
 * while the left column shows how the last minute or so has gone.
 */
export function SettingsPond() {
  const [settings, setSettings] = useState<FrogSettings>(DEFAULT_SETTINGS);
  const [stats, setStats] = useState<PondStats | null>(null);
  const [history, setHistory] = useState<{ stress: number; focus: number }[]>([]);
  const [start, setStart] = useState({ x: 300, y: 120 });
  const [jumpKey, setJumpKey] = useState(0);

  // Settings: load and keep in sync with the tray menu.
  useEffect(() => {
    let live = true;
    void loadSettings()
      .then((next) => {
        if (live) setSettings(next);
      })
      .catch(() => {
        // Backend unavailable (browser preview): keep defaults.
      });
    const unlisten = watchSettings((next) => setSettings(next));
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

  // Hop in from the frog's current screen position.
  const hopIn = useCallback(async () => {
    try {
      const win = getCurrentWindow();
      const [origin, scale] = await Promise.all([
        win.outerPosition(),
        win.scaleFactor(),
      ]);
      const anchor = await invoke<{ x: number; y: number } | null>("frog_anchor");
      if (anchor) {
        const localX = (anchor.x - origin.x) / scale - FROG_SIZE / 2;
        const localY = (anchor.y - origin.y) / scale - FROG_SIZE * 0.8;
        setStart({
          x: Math.max(-140, Math.min(PANEL_W - 40, localX)),
          y: Math.max(-140, Math.min(PANEL_H - 40, localY)),
        });
      }
    } catch {
      // Keep the default start point.
    }
    setJumpKey((key) => key + 1);
  }, []);

  useEffect(() => {
    void hopIn();
  }, [hopIn]);

  // The panel is hidden, not unmounted, so replay the hop each time it reopens.
  useEffect(() => {
    const unlisten = listen("pond-panel-shown", () => {
      void hopIn();
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, [hopIn]);

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

  const stress = stats?.stress ?? 0;
  const focus = stats?.focus ?? 0;
  const stressHistory = history.map((sample) => sample.stress);
  const focusHistory = history.map((sample) => sample.focus);

  return (
    <div
      className="relative h-full w-full overflow-hidden"
      style={{
        backgroundImage: `url(${lilypadPond})`,
        backgroundSize: "cover",
        backgroundPosition: "center",
        fontFamily: '"VT323", ui-monospace, monospace',
      }}
    >
      {/* Status column */}
      <div className="pixel-hud absolute bottom-4 left-4 top-4 flex w-[236px] flex-col px-3 pb-2 pt-0">
        <div className="-mx-3 flex items-center justify-between bg-[#04170f] px-3 py-1.5">
          <span className="text-[15px] uppercase tracking-[0.12em] text-emerald-100">
            Pond status
          </span>
          <span
            className={`text-[11px] uppercase tracking-[0.08em] ${
              settings.nudgesPaused ? "text-amber-300" : "text-emerald-300/80"
            }`}
          >
            {settings.nudgesPaused ? "paused" : "live"}
          </span>
        </div>

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
          <StatRow
            label="idle"
            value={`${stats?.secondsSinceInput ?? 0}s`}
          />
          <StatRow label="in front" value={stats?.activeApp ?? "-"} />
        </div>

        <p className="mt-1 text-[11px] leading-[1.15] text-emerald-100/50">
          anxiety is the pond ripple score; focus falls when you hop between
          windows or drift into a feed.
        </p>
      </div>

      {/* Frog hopping in from his desktop spot onto a lilypad */}
      <motion.div
        key={jumpKey}
        className="pointer-events-none absolute left-0 top-0"
        initial={{ x: start.x, y: start.y }}
        animate={{
          x: [start.x, (start.x + LAND.x) / 2, LAND.x],
          y: [start.y, Math.min(start.y, LAND.y) - 120, LAND.y],
        }}
        transition={{ duration: 1.25, times: [0, 0.55, 1], ease: "easeInOut" }}
      >
        <PondFrog />
      </motion.div>

      {/* Lilypad menu */}
      <div className="absolute right-[4%] top-[8%] text-right">
        <p className="text-[22px] uppercase leading-none tracking-[0.14em] text-emerald-50 drop-shadow-[2px_2px_0_rgba(4,23,15,0.8)]">
          The pond
        </p>
        <p className="text-[14px] text-emerald-100/80 drop-shadow-[1px_1px_0_rgba(4,23,15,0.8)]">
          tap a lilypad
        </p>
      </div>

      <button
        type="button"
        onClick={setPaused}
        className="pond-button absolute left-[56%] top-[36%] -translate-x-1/2 -translate-y-1/2 px-3 py-1.5 text-[15px] uppercase tracking-[0.06em]"
      >
        {settings.nudgesPaused ? "resume nudges" : "pause nudges"}
      </button>

      <button
        type="button"
        onClick={setSound}
        className="pond-button absolute left-[78%] top-[52%] -translate-x-1/2 -translate-y-1/2 px-3 py-1.5 text-[15px] uppercase tracking-[0.06em]"
      >
        croaks: {settings.soundEnabled ? "on" : "off"}
      </button>

      <button
        type="button"
        onClick={close}
        className="pond-button absolute left-[64%] top-[76%] -translate-x-1/2 -translate-y-1/2 px-3 py-1.5 text-[15px] uppercase tracking-[0.06em]"
      >
        back to desktop
      </button>

      <p className="absolute bottom-2 right-3 text-[11px] uppercase tracking-[0.08em] text-emerald-100/50">
        esc closes the pond
      </p>
    </div>
  );
}
