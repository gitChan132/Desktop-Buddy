import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useRef, useState } from "react";
import { playCroak } from "../croak";
import {
  DEFAULT_SETTINGS,
  loadSettings,
  saveNudgesPaused,
  saveSoundEnabled,
  saveVolume,
  watchSettings,
  type FrogSettings,
} from "../settings";

const appWindow = getCurrentWindow();

const DEFAULTS: FrogSettings = DEFAULT_SETTINGS;

type ToggleRowProps = {
  label: string;
  hint: string;
  on: boolean;
  onChange: () => void;
};

function ToggleRow({ label, hint, on, onChange }: ToggleRowProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      onClick={onChange}
      className="flex items-center justify-between gap-2 border-b-2 border-emerald-900/15 py-2 text-left"
    >
      <span>
        <span className="block text-[16px] uppercase leading-none tracking-[0.06em]">
          {label}
        </span>
        <span className="mt-1 block text-[13px] leading-[1.1] text-emerald-800/70">
          {hint}
        </span>
      </span>
      <span className={`pixel-switch ${on ? "is-on" : ""}`} aria-hidden>
        <span />
      </span>
    </button>
  );
}

/** Menu-bar / tray popover: sound, nudges pause, and quit for the frog. */
export function ControlPanel() {
  const [settings, setSettings] = useState<FrogSettings>(DEFAULTS);

  useEffect(() => {
    let live = true;
    void loadSettings()
      .then((next) => {
        if (live) setSettings(next);
      })
      .catch(() => {
        // Backend unavailable (plain browser preview): keep defaults.
      });
    const unlisten = watchSettings((next) => setSettings(next));
    return () => {
      live = false;
      void unlisten.then((off) => off());
    };
  }, []);

  // Popover behaviour: close when the user clicks anywhere else or hits Escape.
  // Only hide after the window has actually taken focus once, so a show/focus
  // race at open time cannot dismiss the panel immediately.
  const hadFocus = useRef(false);
  useEffect(() => {
    const unlisten = appWindow.onFocusChanged(({ payload }) => {
      if (payload) {
        hadFocus.current = true;
      } else if (hadFocus.current) {
        void appWindow.hide();
      }
    });
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") void appWindow.hide();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      void unlisten.then((off) => off());
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  const setPaused = () => {
    const nudgesPaused = !settings.nudgesPaused;
    setSettings((prev) => ({ ...prev, nudgesPaused }));
    void saveNudgesPaused(nudgesPaused);
  };

  const setSound = () => {
    const soundEnabled = !settings.soundEnabled;
    setSettings((prev) => ({ ...prev, soundEnabled }));
    void saveSoundEnabled(soundEnabled);
  };

  const changeVolume = (volume: number) => {
    setSettings((prev) => ({ ...prev, volume }));
    void saveVolume(volume);
  };

  const percent = Math.round(settings.volume * 100);

  return (
    <div className="flex h-full w-full items-stretch bg-transparent p-1.5">
      <div className="pixel-panel relative flex w-full flex-col px-3 py-2.5 text-emerald-950">
        <div className="-mx-3 -mt-2.5 mb-2 flex items-center justify-between bg-[#0b3a1f] px-2 py-1">
          <span className="text-[14px] uppercase leading-none tracking-[0.12em] text-emerald-100">
            For Frog's Sake
          </span>
          <button
            type="button"
            onClick={() => void appWindow.hide()}
            aria-label="Close the control panel"
            className="pixel-button px-1.5 py-0.5 text-[12px] uppercase leading-none text-emerald-900"
          >
            Close
          </button>
        </div>

        <ToggleRow
          label="Croak sounds"
          hint="The little voice under every nudge"
          on={settings.soundEnabled}
          onChange={setSound}
        />
        <ToggleRow
          label="Pause nudges"
          hint="Frog stays on screen, just stops nudging"
          on={settings.nudgesPaused}
          onChange={setPaused}
        />

        <div className="mt-2">
          <div className="flex items-center justify-between text-[15px] uppercase leading-none tracking-[0.06em]">
            <span>Croak volume</span>
            <span className="pixel-chip px-1.5 py-0.5 text-[13px] text-emerald-50">
              {percent}%
            </span>
          </div>
          <input
            type="range"
            min={0}
            max={100}
            step={5}
            value={percent}
            disabled={!settings.soundEnabled}
            aria-label="Croak volume"
            onChange={(event) => changeVolume(Number(event.target.value) / 100)}
            className="pixel-range mt-2 w-full disabled:opacity-40"
          />
        </div>

        <div className="mt-2 flex gap-1.5">
          <button
            type="button"
            onClick={() => void invoke("open_pond", { tab: "activities" })}
            className="pixel-button flex-1 px-2 py-1 text-[13px] uppercase leading-none text-emerald-900"
          >
            Activities
          </button>
          <button
            type="button"
            onClick={() => void invoke("open_pond", { tab: "modes" })}
            className="pixel-button flex-1 px-2 py-1 text-[13px] uppercase leading-none text-emerald-900"
          >
            Modes
          </button>
          <button
            type="button"
            onClick={() => void invoke("open_remote")}
            className="pixel-button flex-1 px-2 py-1 text-[13px] uppercase leading-none text-emerald-900"
          >
            Test
          </button>
        </div>

        <button
          type="button"
          onClick={() => void invoke("open_pond", { tab: "status" })}
          className="pixel-button mt-1.5 w-full px-2 py-1 text-[13px] uppercase leading-none text-emerald-900"
        >
          Pond settings
        </button>

        <div className="mt-auto flex items-center gap-1.5 pt-3">
          <button
            type="button"
            onClick={() => playCroak()}
            className="pixel-button flex-1 px-2 py-1 text-[14px] uppercase leading-none text-emerald-900"
          >
            Test croak
          </button>
          <button
            type="button"
            onClick={() => void invoke("quit_app")}
            className="pixel-button px-2 py-1 text-[14px] uppercase leading-none text-emerald-900"
          >
            Quit
          </button>
        </div>
      </div>
    </div>
  );
}
