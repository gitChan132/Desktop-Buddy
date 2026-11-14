import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useRef, useState } from "react";
import type { FrogInterventionPayload } from "../types";

type BreathPhase = "inhale" | "hold" | "exhale";

const PHASE_MS: Record<BreathPhase, number> = {
  inhale: 4000,
  hold: 4000,
  exhale: 6000,
};

const ACTION_LABEL: Record<FrogInterventionPayload["suggestedAction"], string> =
  {
    belly_breathe: "Belly breathing",
    lily_pad_step: "Lily pad step",
    pond_stretch: "Pond stretch",
  };

interface LilyPadGroundingProps {
  open: boolean;
  suggestedAction: FrogInterventionPayload["suggestedAction"];
  onClose: () => void;
}

/** Soft Web Audio pad: two detuned sines through a low-pass filter. */
function startSynth(): AudioContext | null {
  try {
    const ctx = new AudioContext();
    const gain = ctx.createGain();
    const filter = ctx.createBiquadFilter();
    filter.type = "lowpass";
    filter.frequency.value = 900;
    gain.gain.value = 0;

    const freqs = [174, 261.63, 349.23];
    const oscillators = freqs.map((freq, i) => {
      const osc = ctx.createOscillator();
      osc.type = "sine";
      osc.frequency.value = freq;
      osc.detune.value = i * 4;
      osc.connect(gain);
      osc.start();
      return osc;
    });

    gain.connect(filter);
    filter.connect(ctx.destination);
    gain.gain.linearRampToValueAtTime(0.045, ctx.currentTime + 1.5);

    void ctx.resume();

    // Keep refs alive on the context for teardown.
    (ctx as AudioContext & { __oscillators?: OscillatorNode[] }).__oscillators =
      oscillators;
    return ctx;
  } catch {
    return null;
  }
}

function stopSynth(ctx: AudioContext | null) {
  if (!ctx) return;
  const extra = (ctx as AudioContext & { __oscillators?: OscillatorNode[] })
    .__oscillators;
  void ctx.close();
  extra?.forEach((osc) => {
    try {
      osc.stop();
    } catch {
      /* already stopped */
    }
  });
}

export function LilyPadGrounding({
  open,
  suggestedAction,
  onClose,
}: LilyPadGroundingProps) {
  const [phase, setPhase] = useState<BreathPhase>("inhale");
  const [cycles, setCycles] = useState(0);
  const audioRef = useRef<AudioContext | null>(null);

  // Cycle the breath guide while the modal is open.
  useEffect(() => {
    if (!open) {
      setPhase("inhale");
      setCycles(0);
      return;
    }
    const timer = setTimeout(() => {
      if (phase === "inhale") setPhase("hold");
      else if (phase === "hold") setPhase("exhale");
      else {
        setPhase("inhale");
        setCycles((c) => c + 1);
      }
    }, PHASE_MS[phase]);
    return () => clearTimeout(timer);
  }, [open, phase]);

  // Synth lifecycle.
  useEffect(() => {
    if (!open) {
      stopSynth(audioRef.current);
      audioRef.current = null;
      return;
    }
    audioRef.current ??= startSynth();
    return () => {
      stopSynth(audioRef.current);
      audioRef.current = null;
    };
  }, [open]);

  const scale = phase === "inhale" ? 1.28 : phase === "hold" ? 1.28 : 0.82;

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed inset-0 z-50 flex items-end justify-center bg-emerald-950/35 p-3 backdrop-blur-sm"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          data-interactive
        >
          <motion.div
            initial={{ y: 40, scale: 0.96, opacity: 0 }}
            animate={{ y: 0, scale: 1, opacity: 1 }}
            exit={{ y: 30, scale: 0.96, opacity: 0 }}
            transition={{ type: "spring", stiffness: 280, damping: 26 }}
            className="w-full max-w-[290px] rounded-3xl border border-emerald-800/15 bg-[#f2fbf1] p-4 shadow-2xl shadow-emerald-950/25"
          >
            <div className="flex items-center justify-between">
              <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-emerald-700">
                {ACTION_LABEL[suggestedAction]}
              </p>
              <button
                type="button"
                onClick={onClose}
                className="rounded-full px-2 py-0.5 text-[12px] font-semibold text-emerald-700 transition hover:bg-emerald-900/10"
              >
                Close
              </button>
            </div>

            <div className="relative mt-3 flex h-[190px] items-center justify-center">
              {/* Lily pad ring */}
              <div className="absolute h-[154px] w-[154px] rounded-full bg-emerald-400/25" />
              <motion.div
                animate={{ scale, opacity: phase === "hold" ? 0.85 : 1 }}
                transition={{
                  duration: PHASE_MS[phase] / 1000,
                  ease: "easeInOut",
                }}
                className="flex h-[132px] w-[132px] items-center justify-center rounded-full bg-gradient-to-br from-emerald-300 to-emerald-500 shadow-lg shadow-emerald-700/30"
              >
                <span className="text-[13px] font-bold uppercase tracking-[0.18em] text-emerald-950">
                  {phase === "inhale"
                    ? "Breathe in"
                    : phase === "hold"
                      ? "Hold"
                      : "Breathe out"}
                </span>
              </motion.div>
            </div>

            <p className="mt-2 text-center text-[11px] text-emerald-800">
              {cycles === 0
                ? "4 in · 4 hold · 6 out"
                : `${cycles} ${cycles === 1 ? "cycle" : "cycles"} complete`}
            </p>
            <p className="mt-1 text-center text-[10px] text-emerald-700/70">
              *ribbit* The pond is in no hurry.
            </p>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
