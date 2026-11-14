import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Mirror of `Settings` in `src-tauri/src/settings.rs`. */
export type FrogSettings = {
  nudgesPaused: boolean;
  soundEnabled: boolean;
  volume: number;
};

export const SETTINGS_EVENT = "frog-settings";

export const loadSettings = () => invoke<FrogSettings>("get_settings");

export const saveNudgesPaused = (paused: boolean) =>
  invoke("set_nudges_paused", { paused });

export const saveSoundEnabled = (enabled: boolean) =>
  invoke("set_sound_enabled", { enabled });

export const saveVolume = (volume: number) =>
  invoke("set_sound_volume", { volume });

/** Subscribes to live settings changes from the panel or the tray menu. */
export const watchSettings = (onChange: (settings: FrogSettings) => void) =>
  listen<FrogSettings>(SETTINGS_EVENT, (event) => onChange(event.payload));

export type { UnlistenFn };
