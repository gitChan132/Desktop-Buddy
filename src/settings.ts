import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { FrogColor } from "./sprites";

/** Mirror of `Settings` in `src-tauri/src/settings.rs`. */
export type TriggerId =
  | "frantic_typing"
  | "backspace_loop"
  | "tab_thrashing"
  | "idle_freeze"
  | "cursor_hesitation"
  | "cursor_oscillation"
  | "doomscroll";

export type ActivityId =
  | "belly_breathe"
  | "visual_grounding"
  | "task_intent"
  | "mindful_pause";

export type PondMode = "focus" | "chill" | "fun";

export type FrogSettings = {
  nudgesPaused: boolean;
  soundEnabled: boolean;
  volume: number;
  mode: PondMode;
  frogColor: FrogColor;
  selectionSummaries: boolean;
  triggers: {
    franticTyping: boolean;
    backspaceLoop: boolean;
    tabThrashing: boolean;
    idleFreeze: boolean;
    cursorHesitation: boolean;
    cursorOscillation: boolean;
    doomscroll: boolean;
  };
  activities: {
    bellyBreathe: boolean;
    visualGrounding: boolean;
    taskIntent: boolean;
    mindfulPause: boolean;
  };
};

export const SETTINGS_EVENT = "frog-settings";

export const DEFAULT_SETTINGS: FrogSettings = {
  nudgesPaused: false,
  soundEnabled: true,
  volume: 0.5,
  mode: "focus",
  frogColor: "green",
  selectionSummaries: true,
  triggers: {
    franticTyping: true,
    backspaceLoop: true,
    tabThrashing: true,
    idleFreeze: true,
    cursorHesitation: true,
    cursorOscillation: true,
    doomscroll: true,
  },
  activities: {
    bellyBreathe: true,
    visualGrounding: true,
    taskIntent: true,
    mindfulPause: true,
  },
};

export const loadSettings = () => invoke<FrogSettings>("get_settings");

export const saveNudgesPaused = (paused: boolean) =>
  invoke("set_nudges_paused", { paused });

export const saveSoundEnabled = (enabled: boolean) =>
  invoke("set_sound_enabled", { enabled });

export const saveVolume = (volume: number) =>
  invoke("set_sound_volume", { volume });

export const saveMode = (mode: PondMode) => invoke("set_mode", { mode });

export const saveFrogColor = (color: FrogColor) =>
  invoke("set_frog_color", { color });

export const saveSelectionSummaries = (enabled: boolean) =>
  invoke("set_selection_summaries", { enabled });

export const saveTriggerEnabled = (trigger: TriggerId, enabled: boolean) =>
  invoke("set_trigger_enabled", { trigger, enabled });

export const saveActivityEnabled = (activity: ActivityId, enabled: boolean) =>
  invoke("set_activity_enabled", { activity, enabled });

/** Subscribes to live settings changes from the pond, popover or tray menu. */
export const watchSettings = (onChange: (settings: FrogSettings) => void) =>
  listen<FrogSettings>(SETTINGS_EVENT, (event) => onChange(event.payload));

export type { UnlistenFn };
