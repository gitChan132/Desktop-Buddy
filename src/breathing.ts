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
  { key: "inhale", ms: 6000, label: "breathe in" },
  { key: "hold", ms: 6000, label: "hold" },
  { key: "exhale", ms: 9000, label: "breathe out" },
];
