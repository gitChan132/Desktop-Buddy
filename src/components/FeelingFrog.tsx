import { PhysicalPosition } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AnimatePresence,
  motion,
  useAnimationControls,
} from "framer-motion";
import { useEffect, useRef, useState } from "react";
import greenSpritesheet from "../assets/frog/green_spritesheet.png";
import type { FrogInterventionPayload } from "../types";

/** Pointer travel (CSS px) before a press is treated as a drag instead of a click. */
const DRAG_THRESHOLD = 4;

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

// The sheet is a 16x16 grid of 32px cells. Each animation owns a 4-column
// block, but only its first row is the front-facing loop; the rows below are
// the same motion drawn from other angles, and playing them in sequence makes
// the frog spin on the spot. So each animation below is the one row of frames
// it needs, played left to right.
const SHEET_COLUMNS = 16;
const RENDER_SIZE = 192;

type AnimationName = "idle" | "croak" | "jump" | "hop" | "shock";

const ANIMATIONS: Record<
  AnimationName,
  { column: number; row: number; frames: number }
> = {
  idle: { column: 0, row: 0, frames: 4 },
  croak: { column: 4, row: 0, frames: 4 },
  jump: { column: 8, row: 0, frames: 4 },
  hop: { column: 12, row: 0, frames: 4 },
  shock: { column: 0, row: 8, frames: 5 },
};

/** Steps through one animation row at a fixed frame rate. */
function useSpriteFrame(animation: AnimationName, framesPerSecond: number) {
  const { frames } = ANIMATIONS[animation];
  const [frame, setFrame] = useState(0);

  useEffect(() => {
    setFrame(0);
    const id = setInterval(
      () => setFrame((current) => (current + 1) % frames),
      1000 / framesPerSecond,
    );
    return () => clearInterval(id);
  }, [animation, framesPerSecond, frames]);

  return frame;
}

export function FeelingFrog({
  mood,
  intervention,
  onGrounding,
}: FeelingFrogProps) {
  const hop = useAnimationControls();
  const lastId = useRef<string | null>(null);

  // Drag state: window origin + pointer origin captured on press, so the
  // window follows the cursor without drifting. Null when not dragging.
  const drag = useRef<{
    pointerX: number;
    pointerY: number;
    windowX: number;
    windowY: number;
    scale: number;
    moved: boolean;
  } | null>(null);

  const handlePointerDown = async (event: React.PointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return;
    try {
      const win = getCurrentWindow();
      const [position, scale] = await Promise.all([
        win.outerPosition(),
        win.scaleFactor(),
      ]);
      drag.current = {
        pointerX: event.screenX * scale,
        pointerY: event.screenY * scale,
        windowX: position.x,
        windowY: position.y,
        scale,
        moved: false,
      };
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Window APIs unavailable (plain browser dev): leave dragging disabled.
    }
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLButtonElement>) => {
    const state = drag.current;
    if (!state) return;

    const dx = event.screenX * state.scale - state.pointerX;
    const dy = event.screenY * state.scale - state.pointerY;

    if (!state.moved && Math.hypot(dx, dy) < DRAG_THRESHOLD * state.scale) {
      return;
    }
    state.moved = true;

    void getCurrentWindow()
      .setPosition(
        new PhysicalPosition(
          Math.round(state.windowX + dx),
          Math.round(state.windowY + dy),
        ),
      )
      .catch(() => {});
  };

  const endDrag = (event: React.PointerEvent<HTMLButtonElement>) => {
    if (drag.current && event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  const handleClick = () => {
    const wasDragged = drag.current?.moved ?? false;
    drag.current = null;
    if (wasDragged) return; // A drag must not trigger the breathing panel.
    onGrounding();
  };

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

  const breathing = mood === "breathing";
  const animation: AnimationName = breathing
    ? "idle"
    : mood === "curious"
      ? "croak"
      : "idle";
  // Breathing slows the idle loop right down to match the 7s breath cycle.
  const framesPerSecond = breathing ? 3 : mood === "curious" ? 8 : 6;
  const frame = useSpriteFrame(animation, framesPerSecond);

  const sheetColumn = ANIMATIONS[animation].column + frame;
  const sheetRow = ANIMATIONS[animation].row;

  return (
    <div className="relative h-full w-full">
      <div className="absolute inset-x-0 bottom-[calc(0.75rem+192px)] flex justify-center">
        <AnimatePresence>
          {intervention && !breathing && (
            <motion.div
              key={intervention.id}
              data-interactive
              initial={{ opacity: 0, y: 8, scale: 0.92 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 6, scale: 0.94 }}
              transition={{ type: "spring", stiffness: 320, damping: 24 }}
              className="pointer-events-auto w-[260px] rounded-2xl border border-emerald-900/10 bg-emerald-50/95 px-3.5 py-2.5 text-[12px] leading-snug text-emerald-950 shadow-lg shadow-emerald-950/10 backdrop-blur"
            >
              <p className="font-semibold">{intervention.message}</p>
              <p className="mt-1 text-[11px] text-emerald-700">
                {ACTION_LABEL[intervention.suggestedAction]}
              </p>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      <div className="absolute inset-x-0 bottom-3 flex justify-center">
      <motion.button
        type="button"
        data-interactive
        aria-label="Drag to reposition the frog, click for the lily pad breathing guide"
        onClick={handleClick}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        animate={hop}
        whileHover={{ scale: 1.04 }}
        whileTap={{ scale: 0.95, y: 2 }}
        className="pointer-events-auto relative cursor-grab touch-none select-none bg-transparent outline-none active:cursor-grabbing"
        style={{ width: RENDER_SIZE, height: RENDER_SIZE }}
      >
        {/* Soft contact shadow so the sprite sits on the desktop. */}
        <span
          aria-hidden
          className="absolute bottom-1 left-1/2 h-3 w-3/4 -translate-x-1/2 rounded-full bg-emerald-950/25 blur-[6px]"
        />
        <span
          aria-hidden
          className="absolute inset-0"
          style={{
            backgroundImage: `url(${greenSpritesheet})`,
            backgroundSize: `${SHEET_COLUMNS * RENDER_SIZE}px ${SHEET_COLUMNS * RENDER_SIZE}px`,
            backgroundPosition: `${-sheetColumn * RENDER_SIZE}px ${-sheetRow * RENDER_SIZE}px`,
            backgroundRepeat: "no-repeat",
            imageRendering: "pixelated",
          }}
        />
      </motion.button>
      </div>
    </div>
  );
}
