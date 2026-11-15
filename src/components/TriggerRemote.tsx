import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import {
  DEFAULT_SETTINGS,
  loadSettings,
  watchSettings,
  type FrogSettings,
  type TriggerId,
} from "../settings";

const appWindow = getCurrentWindow();

const ROWS: { id: TriggerId; key: keyof FrogSettings["triggers"]; label: string }[] = [
  { id: "frantic_typing", key: "franticTyping", label: "Frantic typing" },
  { id: "backspace_loop", key: "backspaceLoop", label: "Backspace loop" },
  { id: "tab_thrashing", key: "tabThrashing", label: "Tab thrashing" },
  { id: "idle_freeze", key: "idleFreeze", label: "Idle freeze" },
  { id: "cursor_hesitation", key: "cursorHesitation", label: "Cursor hesitation" },
  { id: "cursor_oscillation", key: "cursorOscillation", label: "Cursor oscillation" },
  { id: "doomscroll", key: "doomscroll", label: "Doomscroll" },
];

/**
 * Small always-on-top remote for demonstrating detectors. It never hides the
 * frog: each button fires a nudge above the real threshold, skips the cooldown,
 * and leaves the bubble up until dismissed.
 */
export function TriggerRemote() {
  const [settings, setSettings] = useState<FrogSettings>(DEFAULT_SETTINGS);
  const [status, setStatus] = useState<string | null>(null);

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

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") void appWindow.hide();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const fire = async (trigger: TriggerId) => {
    setStatus(`Firing ${trigger}…`);
    try {
      await invoke("demo_trigger", { trigger });
      setStatus(null);
    } catch (error) {
      setStatus(String(error));
    }
  };

  const fireRandom = () => {
    const enabled = ROWS.filter((row) => settings.triggers[row.key]);
    const pick = enabled[Math.floor(Math.random() * enabled.length)];
    if (pick) void fire(pick.id);
    else setStatus("All triggers are muted in Settings.");
  };

  return (
    <div className="flex h-full w-full items-stretch bg-transparent p-1.5">
      <div className="pixel-panel relative flex w-full flex-col px-2.5 py-2 text-emerald-950">
        <div className="-mx-2.5 -mt-2 mb-1.5 flex items-center justify-between bg-[#0b3a1f] px-2 py-1">
          <span className="text-[13px] uppercase leading-none tracking-[0.1em] text-emerald-100">
            Trigger remote
          </span>
          <button
            type="button"
            onClick={() => void appWindow.hide()}
            aria-label="Close the trigger remote"
            className="pixel-button px-1.5 py-0.5 text-[11px] uppercase leading-none text-emerald-900"
          >
            Close
          </button>
        </div>

        <div className="flex flex-1 flex-col gap-1 overflow-y-auto pr-0.5">
          {ROWS.map((row) => {
            const on = settings.triggers[row.key];
            return (
              <button
                key={row.id}
                type="button"
                disabled={!on}
                onClick={() => void fire(row.id)}
                className="pixel-button px-2 py-1 text-left text-[13px] uppercase leading-none text-emerald-900 disabled:opacity-40"
              >
                {row.label}
                {!on && (
                  <span className="ml-1 text-[11px] normal-case text-emerald-900/60">
                    (muted)
                  </span>
                )}
              </button>
            );
          })}
          <button
            type="button"
            onClick={fireRandom}
            className="pixel-button px-2 py-1 text-left text-[13px] uppercase leading-none text-emerald-900"
          >
            Random trigger
          </button>
        </div>

        {status && (
          <p className="mt-1.5 text-[11px] leading-[1.1] text-amber-700">{status}</p>
        )}
      </div>
    </div>
  );
}
