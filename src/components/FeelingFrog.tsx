import {
  AnimatePresence,
  motion,
  useAnimationControls,
} from "framer-motion";
import { useEffect, useRef, useState } from "react";
import type { FrogInterventionPayload } from "../types";

export type FrogMood = "idle" | "curious" | "breathing";

interface FeelingFrogProps {
  mood: FrogMood;
  intervention: FrogInterventionPayload | null;
  onGrounding: () => void;
}

const ACTION_LABEL: Record<FrogInterventionPayload["suggestedAction"], string> =
  {
    belly_breathe: "Belly breathe with me",
    lily_pad_step: "Step onto the lily pad",
    pond_stretch: "Pond stretch",
  };

export function FeelingFrog({
  mood,
  intervention,
  onGrounding,
}: FeelingFrogProps) {
  const hop = useAnimationControls();
  const [blink, setBlink] = useState(false);
  const lastId = useRef<string | null>(null);

  // Springy hop whenever a fresh intervention lands.
  useEffect(() => {
    if (!intervention || intervention.id === lastId.current) return;
    lastId.current = intervention.id;
    void hop.start({
      y: [0, -26, -14, 0],
      rotate: [0, -4, 3, 0],
      transition: { type: "spring", stiffness: 260, damping: 12, mass: 0.9 },
    });
  }, [intervention, hop]);

  // Idle blink loop.
  useEffect(() => {
    const id = setInterval(() => {
      setBlink(true);
      setTimeout(() => setBlink(false), 140);
    }, 4200 + Math.round(Math.random() * 2200));
    return () => clearInterval(id);
  }, []);

  const breathing = mood === "breathing";

  return (
    <div className="relative flex flex-col items-center pb-3">
      <AnimatePresence>
        {intervention && !breathing && (
          <motion.div
            key={intervention.id}
            data-interactive
            initial={{ opacity: 0, y: 8, scale: 0.92 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 6, scale: 0.94 }}
            transition={{ type: "spring", stiffness: 320, damping: 24 }}
            className="pointer-events-auto mb-2 max-w-[260px] rounded-2xl border border-emerald-900/10 bg-emerald-50/95 px-3.5 py-2.5 text-[12px] leading-snug text-emerald-950 shadow-lg shadow-emerald-950/10 backdrop-blur"
          >
            <p className="font-semibold">{intervention.message}</p>
            <p className="mt-1 text-[11px] text-emerald-700">
              {ACTION_LABEL[intervention.suggestedAction]}
            </p>
          </motion.div>
        )}
      </AnimatePresence>

      <motion.button
        type="button"
        data-interactive
        aria-label="Open the lily pad breathing guide"
        onClick={onGrounding}
        animate={hop}
        whileHover={{ scale: 1.04 }}
        whileTap={{ scale: 0.95, y: 2 }}
        className="pointer-events-auto relative h-[190px] w-[210px] cursor-pointer bg-transparent outline-none"
      >
        <motion.svg
          viewBox="0 0 210 190"
          className="h-full w-full drop-shadow-[0_10px_18px_rgba(6,78,59,0.25)]"
          animate={breathing ? { scale: [1, 1.04, 1] } : { scale: 1 }}
          transition={
            breathing
              ? { duration: 7, repeat: Infinity, ease: "easeInOut" }
              : { type: "spring", stiffness: 200, damping: 18 }
          }
        >
          {/* Lily pad */}
          <motion.ellipse
            cx="105"
            cy="168"
            rx="86"
            ry="17"
            fill="#2f9e6a"
            animate={{ scaleX: [1, 1.03, 1] }}
            transition={{ duration: 4.5, repeat: Infinity, ease: "easeInOut" }}
            style={{ originX: "50%", originY: "50%" }}
          />
          <path
            d="M105 168 L128 152 L140 168 Z"
            fill="#3fb87e"
            opacity="0.9"
          />

          {/* Back legs */}
          <motion.g
            animate={breathing ? { rotate: [0, -3, 0] } : { rotate: 0 }}
            transition={{ duration: 7, repeat: Infinity, ease: "easeInOut" }}
            style={{ originX: "105px", originY: "140px" }}
          >
            <ellipse cx="55" cy="145" rx="26" ry="14" fill="#3aa86f" />
            <ellipse cx="155" cy="145" rx="26" ry="14" fill="#3aa86f" />
          </motion.g>

          {/* Body with breathing squash */}
          <motion.g
            animate={
              breathing
                ? { scaleY: [1, 1.06, 1], scaleX: [1, 0.97, 1] }
                : { scaleY: 1, scaleX: 1 }
            }
            transition={{ duration: 7, repeat: Infinity, ease: "easeInOut" }}
            style={{ originX: "105px", originY: "130px" }}
          >
            <ellipse cx="105" cy="112" rx="64" ry="52" fill="#45c07f" />
            <ellipse cx="105" cy="124" rx="45" ry="32" fill="#b9ecc9" />

            {/* Front legs */}
            <ellipse cx="55" cy="132" rx="16" ry="22" fill="#3aa86f" />
            <ellipse cx="155" cy="132" rx="16" ry="22" fill="#3aa86f" />

            {/* Eyes */}
            <g>
              <circle cx="76" cy="62" r="23" fill="#45c07f" />
              <circle cx="134" cy="62" r="23" fill="#45c07f" />
              <motion.g animate={{ scaleY: blink ? 0.1 : 1 }} style={{ originY: "64px" }}>
                <circle cx="76" cy="62" r="15" fill="#f7fbf5" />
                <circle cx="134" cy="62" r="15" fill="#f7fbf5" />
                <motion.circle
                  cx="79"
                  cy="64"
                  r="7.5"
                  fill="#123127"
                  animate={
                    breathing
                      ? { cy: [64, 66, 64] }
                      : { cx: [79, 82, 79], cy: [64, 63, 64] }
                  }
                  transition={{ duration: breathing ? 7 : 8, repeat: Infinity, ease: "easeInOut" }}
                />
                <motion.circle
                  cx="137"
                  cy="64"
                  r="7.5"
                  fill="#123127"
                  animate={
                    breathing
                      ? { cy: [64, 66, 64] }
                      : { cx: [137, 140, 137], cy: [64, 63, 64] }
                  }
                  transition={{ duration: breathing ? 7 : 8, repeat: Infinity, ease: "easeInOut" }}
                />
              </motion.g>
              {/* Cheeks */}
              <ellipse cx="55" cy="86" rx="9" ry="5" fill="#f7a8a0" opacity="0.55" />
              <ellipse cx="155" cy="86" rx="9" ry="5" fill="#f7a8a0" opacity="0.55" />
            </g>

            {/* Mouth */}
            <motion.path
              d="M92 96 Q105 106 118 96"
              stroke="#1c5a42"
              strokeWidth="3"
              strokeLinecap="round"
              fill="none"
              animate={{ d: breathing ? "M94 97 Q105 104 116 97" : "M92 96 Q105 106 118 96" }}
              transition={{ duration: 7, repeat: Infinity, ease: "easeInOut" }}
            />
          </motion.g>
        </motion.svg>
      </motion.button>
    </div>
  );
}
