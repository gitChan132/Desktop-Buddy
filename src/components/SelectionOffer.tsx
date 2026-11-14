import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useState } from "react";
import type { SelectionReadyPayload } from "../types";

/** Pill shows for a moment; summaries linger until dismissed. */
const PILL_TTL_MS = 12_000;
const SUMMARY_TTL_MS = 45_000;

type Phase = "idle" | "loading" | "summary" | "error";

/**
 * Offers to summarize long highlighted passages. The backend watches the
 * system-wide selection and hands us the text; we ask first, then show the
 * summary in a paper bubble above the frog.
 */
export function SelectionOffer({
  offer,
  hint,
  onDismiss,
  onDismissHint,
}: {
  offer: SelectionReadyPayload | null;
  /** True when macOS has not granted Accessibility access yet. */
  hint: boolean;
  onDismiss: () => void;
  onDismissHint: () => void;
}) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [body, setBody] = useState("");

  // A fresh selection resets the card back to its offer state.
  useEffect(() => {
    setPhase("idle");
    setBody("");
  }, [offer]);

  // Never leave anything lingering on screen.
  useEffect(() => {
    if (!offer) return;
    if (phase === "idle") {
      const timer = setTimeout(onDismiss, PILL_TTL_MS);
      return () => clearTimeout(timer);
    }
    if (phase === "summary" || phase === "error") {
      const timer = setTimeout(onDismiss, SUMMARY_TTL_MS);
      return () => clearTimeout(timer);
    }
  }, [offer, phase, onDismiss]);

  // Escape closes whatever is up.
  useEffect(() => {
    if (!offer && !hint) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (hint) onDismissHint();
      if (offer) onDismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [offer, hint, onDismiss, onDismissHint]);

  const summarize = async () => {
    if (!offer) return;
    setPhase("loading");
    try {
      const summary = await invoke<string>("summarize_selection", {
        text: offer.text,
      });
      setBody(summary);
      setPhase("summary");
    } catch (error) {
      setBody(String(error));
      setPhase("error");
    }
  };

  const openSettings = () => {
    void invoke("open_accessibility_settings").catch(() => {});
    onDismissHint();
  };

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-[106px] z-10 flex justify-center">
      <AnimatePresence mode="wait">
        {hint && !offer && (
          <motion.div
            key="hint"
            data-interactive
            initial={{ opacity: 0, y: 8, scale: 0.92 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 6, scale: 0.94 }}
            className="pixel-chip pointer-events-auto relative flex max-w-[300px] items-center gap-2 px-2.5 py-1.5 text-[15px] leading-tight text-emerald-50"
          >
            <span>*ribbit* Let me read your highlights?</span>
            <button
              type="button"
              onClick={openSettings}
              className="pixel-button px-1.5 py-0.5 text-[13px] uppercase leading-none text-emerald-950"
            >
              allow
            </button>
            <button
              type="button"
              aria-label="Dismiss"
              onClick={onDismissHint}
              className="text-[15px] leading-none text-emerald-200"
            >
              ✕
            </button>
            <span aria-hidden className="pixel-tail" />
          </motion.div>
        )}

        {offer && phase === "idle" && (
          <motion.div
            key="offer"
            data-interactive
            initial={{ opacity: 0, y: 8, scale: 0.92 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 6, scale: 0.94 }}
            className="pixel-chip pointer-events-auto relative flex max-w-[300px] items-center gap-2 px-2.5 py-1.5 text-[15px] leading-tight text-emerald-50"
          >
            <span>Summarize {offer.words} words?</span>
            <button
              type="button"
              onClick={() => void summarize()}
              className="pixel-button px-1.5 py-0.5 text-[13px] uppercase leading-none text-emerald-950"
            >
              yes
            </button>
            <button
              type="button"
              aria-label="Dismiss"
              onClick={onDismiss}
              className="text-[15px] leading-none text-emerald-200"
            >
              ✕
            </button>
            <span aria-hidden className="pixel-tail" />
          </motion.div>
        )}

        {offer && phase === "loading" && (
          <motion.div
            key="loading"
            data-interactive
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 4 }}
            className="pixel-chip pointer-events-auto relative flex items-center gap-1.5 px-2.5 py-1 text-[15px] uppercase leading-none tracking-[0.06em] text-emerald-50"
          >
            <motion.span
              initial={{ scale: 0.6, opacity: 0.4 }}
              animate={{ scale: [0.8, 1.15, 0.8], opacity: 1 }}
              transition={{ duration: 1.2, repeat: Infinity, ease: "easeInOut" }}
              className="h-2 w-2 bg-emerald-300"
            />
            reading
            <span aria-hidden className="pixel-tail" />
          </motion.div>
        )}

        {offer && (phase === "summary" || phase === "error") && (
          <motion.div
            key="summary"
            data-interactive
            initial={{ opacity: 0, y: 8, scale: 0.92 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 6, scale: 0.94 }}
            className="pixel-panel pointer-events-auto relative w-[300px] px-3 pt-2 pb-3 text-emerald-950"
          >
            <pre className="max-h-[220px] overflow-y-auto whitespace-pre-wrap font-[inherit] text-[16px] leading-[1.2]">
              {body}
            </pre>
            <button
              type="button"
              aria-label="Dismiss"
              onClick={onDismiss}
              className="pixel-button mt-1.5 px-2 py-0.5 text-[13px] uppercase leading-none"
            >
              ok
            </button>
            <span aria-hidden className="pixel-tail" />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
