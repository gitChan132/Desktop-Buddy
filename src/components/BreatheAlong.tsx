import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useState } from "react";
import { BREATH_PHASES, type BreathPhase } from "../breathing";

/**
 * A slow guided breathing exercise, narrated by the frog while he croaks.
 * Inhale / hold / exhale cycle forever, each with its own line of frog talk.
 */
const PHASES = BREATH_PHASES;

/** Two lines per phase: one for the first round, one for later rounds. */
const LINES: Record<BreathPhase, [string, string]> = {
  inhale: [
    "Settle in, then breathe in through your nose — slow and even.",
    "In again, fill up like the pond.",
  ],
  hold: ["Hold it, soft. Nothing clenched.", "Hold… no rush."],
  exhale: [
    "And out, long and quiet. Rrrribbit.",
    "Out again, let your shoulders drop.",
  ],
};

/** Short low croak: a filtered saw that falls in pitch. Best effort only. */
function playCroak() {
  try {
    const ctx = new AudioContext();
    void ctx.resume();

    const now = ctx.currentTime;
    const osc = ctx.createOscillator();
    const filter = ctx.createBiquadFilter();
    const gain = ctx.createGain();

    osc.type = "sawtooth";
    osc.frequency.setValueAtTime(150, now);
    osc.frequency.exponentialRampToValueAtTime(92, now + 0.28);

    filter.type = "lowpass";
    filter.frequency.value = 750;

    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(0.06, now + 0.035);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.32);

    osc.connect(filter);
    filter.connect(gain);
    gain.connect(ctx.destination);
    osc.start(now);
    osc.stop(now + 0.35);
    osc.onended = () => {
      void ctx.close();
    };
  } catch {
    // Audio unavailable (autoplay policy, no output device): stay silent.
  }
}

interface BreatheAlongProps {
  open: boolean;
  onClose: () => void;
  /** Reports the live phase so the frog can mirror the breath. */
  onPhase: (phase: BreathPhase | null) => void;
}

export function BreatheAlong({ open, onClose, onPhase }: BreatheAlongProps) {
  const [phaseIndex, setPhaseIndex] = useState(0);
  const [round, setRound] = useState(1);
  const phase = PHASES[phaseIndex];

  // March through inhale → hold → exhale, counting rounds.
  useEffect(() => {
    if (!open) {
      setPhaseIndex(0);
      setRound(1);
      return;
    }
    const id = setTimeout(() => {
      if (phaseIndex === PHASES.length - 1) {
        setRound((value) => value + 1);
        setPhaseIndex(0);
      } else {
        setPhaseIndex((value) => value + 1);
      }
    }, PHASES[phaseIndex].ms);
    return () => clearTimeout(id);
  }, [open, phaseIndex]);

  // Keep the frog's cheeks in step with the guide.
  useEffect(() => {
    onPhase(open ? phase.key : null);
  }, [open, phase.key, onPhase]);

  // He croaks softly as you breathe out.
  useEffect(() => {
    if (!open) return;
    if (PHASES[phaseIndex]?.key === "exhale") playCroak();
  }, [open, phaseIndex]);

  const line = LINES[phase.key][round === 1 ? 0 : 1];

  // Inhale fills the bar, the hold keeps it full, exhale drains it.
  const barFrom = phase.key === "inhale" ? "0%" : "100%";
  const barTo = phase.key === "exhale" ? "0%" : "100%";

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-[130px] z-50 flex flex-col items-center gap-1.5 px-3">
      <AnimatePresence>
        {open && (
          <motion.div
            data-interactive
            initial={{ opacity: 0, y: 10, scale: 0.96 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 8, scale: 0.97 }}
            transition={{ type: "spring", stiffness: 320, damping: 26 }}
            className="pointer-events-auto w-[292px] rounded-2xl border border-emerald-900/10 bg-emerald-50/95 px-3.5 py-3 text-emerald-950 shadow-lg shadow-emerald-950/15 backdrop-blur"
          >
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-semibold uppercase tracking-[0.16em] text-emerald-700">
                Breathe with me
              </span>
              <button
                type="button"
                onClick={onClose}
                aria-label="Close the breathing guide"
                className="rounded-full px-1.5 text-[12px] font-semibold leading-none text-emerald-700 transition hover:bg-emerald-900/10"
              >
                Close
              </button>
            </div>

            <AnimatePresence mode="wait" initial={false}>
              <motion.p
                key={`${round}-${phase.key}`}
                initial={{ opacity: 0, y: 5 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -5 }}
                transition={{ duration: 0.18 }}
                className="mt-1.5 min-h-[34px] text-[12px] font-medium leading-snug"
              >
                {line}
              </motion.p>
            </AnimatePresence>

            {/* Fills as you breathe in, stays full on the hold, drains on the way out. */}
            <div className="mt-2 h-1 overflow-hidden rounded-full bg-emerald-900/10">
              <motion.div
                key={`${round}-${phase.key}-bar`}
                initial={{ width: barFrom }}
                animate={{ width: barTo }}
                transition={{
                  duration: phase.key === "hold" ? 0 : phase.ms / 1000,
                  ease: "linear",
                }}
                className="h-full rounded-full bg-emerald-500"
              />
            </div>

            <div className="mt-1.5 flex items-center justify-between">
              <div className="flex items-center gap-1">
                {PHASES.map((item, index) => (
                  <span
                    key={item.key}
                    className={`h-1.5 w-1.5 rounded-full transition-colors ${
                      index <= phaseIndex ? "bg-emerald-600" : "bg-emerald-900/15"
                    }`}
                  />
                ))}
              </div>
              <span className="text-[10px] text-emerald-700">
                {PHASES.map((item) => item.ms / 1000).join(" · ")} · round{" "}
                {round}
              </span>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {open && (
          <motion.div
            key="status"
            data-interactive
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 4 }}
            transition={{ duration: 0.2 }}
            className="pointer-events-auto relative flex items-center gap-1.5 rounded-full bg-emerald-900/85 px-2.5 py-1 text-[10px] font-medium text-emerald-50 shadow-md shadow-emerald-950/25"
          >
            <motion.span
              key={`${round}-${phase.key}-dot`}
              initial={{ scale: 0.6, opacity: 0.4 }}
              animate={{ scale: [0.8, 1.15, 0.8], opacity: 1 }}
              transition={{
                duration: phase.key === "hold" ? 2 : phase.ms / 1000,
                repeat: Infinity,
                ease: "easeInOut",
              }}
              className="h-1.5 w-1.5 rounded-full bg-emerald-300"
            />
            {phase.label}
            <span className="absolute -bottom-1 left-1/2 h-2 w-2 -translate-x-1/2 rotate-45 bg-emerald-900/85" />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
