import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useRef, useState } from "react";
import { playCroak } from "../croak";

/**
 * A 30-second mindful pause: hands off, eyes closed if comfortable, watch the
 * breath without steering it, let attention settle.
 */
interface PauseStep {
  ms: number;
  label: string;
  line: string;
}

const STEPS: PauseStep[] = [
  {
    ms: 8000,
    label: "hands off",
    line: "Stop typing and using the mouse. Let your hands rest.",
  },
  {
    ms: 8000,
    label: "soft eyes",
    line: "Close your eyes if that's comfortable — or soften your gaze.",
  },
  {
    ms: 8000,
    label: "just notice",
    line: "Notice your breathing. Don't try to change it; just follow one breath in and out.",
  },
  {
    ms: 6000,
    label: "settle",
    line: "Let your attention settle, like silt sinking to the bottom of the pond.",
  },
];

const TOTAL_MS = STEPS.reduce((sum, step) => sum + step.ms, 0);

/** Index of the step covering `elapsed` ms. */
function stepAt(elapsed: number) {
  let acc = 0;
  for (let index = 0; index < STEPS.length; index += 1) {
    acc += STEPS[index].ms;
    if (elapsed < acc) return index;
  }
  return STEPS.length - 1;
}

interface MindfulPauseProps {
  open: boolean;
  onClose: () => void;
}

export function MindfulPause({ open, onClose }: MindfulPauseProps) {
  const [elapsed, setElapsed] = useState(0);
  // Ref so a parent re-render cannot restart the timer.
  const onCloseRef = useRef(onClose);

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    setElapsed(0);
    if (!open) return;
    const started = performance.now();
    const id = setInterval(() => {
      const next = performance.now() - started;
      if (next >= TOTAL_MS) {
        clearInterval(id);
        setElapsed(TOTAL_MS);
        onCloseRef.current();
        return;
      }
      setElapsed(next);
    }, 100);
    return () => clearInterval(id);
  }, [open]);

  const stepIndex = stepAt(elapsed);
  const step = STEPS[stepIndex];

  // He croaks as each new line lands.
  useEffect(() => {
    if (open) playCroak();
  }, [open, stepIndex]);

  const remaining = Math.max(1, Math.ceil((TOTAL_MS - elapsed) / 1000));

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
            className="pointer-events-auto pixel-panel w-[292px] px-3 py-2.5 text-emerald-950"
          >
            <div className="-mx-3 -mt-2.5 mb-2 flex items-center justify-between bg-[#0b3a1f] px-2 py-1">
              <span className="text-[14px] uppercase leading-none tracking-[0.12em] text-emerald-100">
                Mindful pause
              </span>
              <button
                type="button"
                onClick={onClose}
                aria-label="Close the mindful pause"
                className="pixel-button px-1.5 py-0.5 text-[12px] uppercase leading-none text-emerald-900"
              >
                Close
              </button>
            </div>

            <AnimatePresence mode="wait" initial={false}>
              <motion.p
                key={stepIndex}
                initial={{ opacity: 0, y: 5 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -5 }}
                transition={{ duration: 0.18 }}
                className="mt-1 min-h-[42px] text-[18px] leading-[1.15]"
              >
                {step.line}
              </motion.p>
            </AnimatePresence>

            {/* One bar per line: fills over that step, then restarts with the text. */}
            <div className="pixel-bar mt-1.5 h-2.5 overflow-hidden">
              <motion.div
                key={stepIndex}
                initial={{ width: "0%" }}
                animate={{ width: "100%" }}
                transition={{ duration: step.ms / 1000, ease: "linear" }}
                className="h-full bg-emerald-500"
              />
            </div>

            <div className="mt-1.5 flex items-center justify-between">
              <div className="flex items-center gap-1">
                {STEPS.map((item, index) => (
                  <span
                    key={item.label}
                    className={`h-2 w-2 transition-colors ${
                      index <= stepIndex ? "bg-emerald-600" : "bg-emerald-900/15"
                    }`}
                  />
                ))}
              </div>
              <span className="text-[14px] leading-none text-emerald-800">
                {remaining}s left
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
            className="pixel-chip pointer-events-auto relative flex items-center gap-1.5 px-2.5 py-1 text-[15px] uppercase leading-none tracking-[0.06em] text-emerald-50"
          >
            <motion.span
              key={`${stepIndex}-dot`}
              initial={{ scale: 0.6, opacity: 0.4 }}
              animate={{ scale: [0.8, 1.15, 0.8], opacity: 1 }}
              transition={{ duration: 2, repeat: Infinity, ease: "easeInOut" }}
              className="h-2 w-2 bg-emerald-300"
            />
            {step.label}
            <span aria-hidden className="pixel-tail" />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
