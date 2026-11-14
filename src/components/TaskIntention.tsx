import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import { playCroak } from "../croak";

/**
 * One-task intention setting: name the single next priority, then sit with it
 * for a short focus beat while everything else is explicitly parked.
 */
const HOLD_MS = 20_000;

interface TaskIntentionProps {
  open: boolean;
  onClose: () => void;
}

export function TaskIntention({ open, onClose }: TaskIntentionProps) {
  const [text, setText] = useState("");
  const [task, setTask] = useState<string | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  // Held in a ref so parent re-renders cannot restart the hold timer.
  const onCloseRef = useRef(onClose);

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  // Fresh prompt each time it opens, with the field ready to type into.
  useEffect(() => {
    if (!open) {
      setText("");
      setTask(null);
      setElapsed(0);
      return;
    }
    const focus = setTimeout(() => inputRef.current?.focus(), 80);
    return () => clearTimeout(focus);
  }, [open]);

  // Once the priority is named, hold it on screen for a short focus beat.
  useEffect(() => {
    if (!open || task === null) return;
    const started = performance.now();
    const id = setInterval(() => {
      const next = performance.now() - started;
      if (next >= HOLD_MS) {
        clearInterval(id);
        setElapsed(HOLD_MS);
        onCloseRef.current();
        return;
      }
      setElapsed(next);
    }, 100);
    return () => clearInterval(id);
  }, [open, task]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const trimmed = text.trim();
    if (!trimmed) return;
    setElapsed(0);
    setTask(trimmed);
    playCroak();
  };

  const remaining = Math.max(0, Math.ceil((HOLD_MS - elapsed) / 1000));

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
            onKeyDown={(event) => {
              if (event.key === "Escape") onClose();
            }}
          >
            <div className="-mx-3 -mt-2.5 mb-2 flex items-center justify-between bg-[#0b3a1f] px-2 py-1">
              <span className="text-[14px] uppercase leading-none tracking-[0.12em] text-emerald-100">
                One thing
              </span>
              <button
                type="button"
                onClick={onClose}
                aria-label="Close the intention setting"
                className="pixel-button px-1.5 py-0.5 text-[12px] uppercase leading-none text-emerald-900"
              >
                Close
              </button>
            </div>

            <AnimatePresence mode="wait" initial={false}>
              {task === null ? (
                <motion.div
                  key="prompt"
                  initial={{ opacity: 0, y: 5 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -5 }}
                  transition={{ duration: 0.18 }}
                >
                  <p className="text-[18px] leading-[1.15]">
                    What is one thing I need to accomplish right now?
                  </p>
                  <form onSubmit={submit} className="mt-2 flex items-center gap-1.5">
                    <input
                      ref={inputRef}
                      value={text}
                      onChange={(event) => setText(event.target.value)}
                      maxLength={80}
                      placeholder="the next real step"
                      aria-label="Your one priority"
                      className="min-w-0 flex-1 border-2 border-emerald-900 bg-emerald-50/80 px-1.5 py-0.5 font-[inherit] text-[17px] leading-none text-emerald-950 outline-none placeholder:text-emerald-900/40"
                    />
                    <button
                      type="submit"
                      className="pixel-button px-2 py-0.5 text-[14px] uppercase leading-none text-emerald-900"
                    >
                      Set it
                    </button>
                  </form>
                  <p className="mt-1.5 text-[14px] leading-none text-emerald-800">
                    Everything else can wait.
                  </p>
                </motion.div>
              ) : (
                <motion.div
                  key="focus"
                  initial={{ opacity: 0, y: 5 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -5 }}
                  transition={{ duration: 0.18 }}
                >
                  <p className="min-h-[42px] text-[18px] leading-[1.15]">
                    “{task}” — that is the one thing right now. Everything
                    else is parked.
                  </p>
                  <div className="pixel-bar mt-1.5 h-2.5 overflow-hidden">
                    <motion.div
                      initial={{ width: "100%" }}
                      animate={{ width: "0%" }}
                      transition={{ duration: HOLD_MS / 1000, ease: "linear" }}
                      className="h-full bg-emerald-500"
                    />
                  </div>
                  <div className="mt-1.5 flex items-center justify-between">
                    <span className="text-[14px] leading-none text-emerald-800">
                      one task at a time
                    </span>
                    <span className="text-[14px] leading-none text-emerald-800">
                      {remaining}s
                    </span>
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
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
              key={task === null ? "prompt" : "focus"}
              initial={{ scale: 0.6, opacity: 0.4 }}
              animate={{ scale: [0.8, 1.15, 0.8], opacity: 1 }}
              transition={{ duration: 2, repeat: Infinity, ease: "easeInOut" }}
              className="h-2 w-2 bg-emerald-300"
            />
            {task === null ? "name it" : "only this"}
            <span aria-hidden className="pixel-tail" />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
