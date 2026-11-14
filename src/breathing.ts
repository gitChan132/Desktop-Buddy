/** Shared breathing rhythm, used by both the guide panel and the frog sprite. */
export type BreathPhase = "inhale" | "hold" | "exhale";

export interface BreathStep {
  key: BreathPhase;
  /** How long the phase lasts. */
  ms: number;
  /** Live cue shown under the frog. */
  label: string;
}

export const BREATH_PHASES: BreathStep[] = [
  { key: "inhale", ms: 3000, label: "breathe in" },
  { key: "hold", ms: 3000, label: "hold" },
  { key: "exhale", ms: 5000, label: "breathe out" },
];
