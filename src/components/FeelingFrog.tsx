import { PhysicalPosition } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import { cursorPosition, getCurrentWindow } from "@tauri-apps/api/window";
import {
  AnimatePresence,
  motion,
  useAnimationControls,
} from "framer-motion";
import { useEffect, useRef, useState } from "react";
import greenSpritesheet from "../assets/frog/green_spritesheet.png";
import { INTERVENTION_TTL_MS, type FrogInterventionPayload } from "../types";

/**
 * Where a hand would grip the frog inside one 32px cell: the centre of the
 * crown, just below the top of the head in the extended jump pose. The window
 * is shifted so this exact point sits under the cursor while carrying him.
 */
const CROWN_X = 15.5 / 32;
const CROWN_Y = 13 / 32;

/**
 * Frames of the jump row where the frog is stretched out nose-to-toes. While
 * it is being carried the two alternate, so the held pose keeps dangling
 * rather than freezing mid-air.
 */
const HELD_FRAMES = [1, 2];
const HELD_FRAMES_PER_SECOND = 2;

/**
 * Airborne poses. Shock frame 1 is the one with wide, startled eyes and a
 * fully stretched body; jump frame 2 is the longest jump stretch. He holds
 * the wide-eyed pose and only briefly flexes into the jump stretch, so a
 * throw reads as one stiff startled arc instead of a fast animation loop.
 * `hold` is how long that pose stays up, in ms.
 */
const FLIGHT_FRAMES = [
  { column: 1, row: 8, hold: 1400 },
  { column: 10, row: 0, hold: 420 },
];

/** Pointer travel (CSS px) before a press is treated as a drag instead of a click. */
const DRAG_THRESHOLD = 4;

// Throw physics, in physical pixels and seconds.
/** Downward pull so a thrown frog arcs back to the desktop. */
const THROW_GRAVITY = 2600;
/** Exponential air drag; higher stops him sooner. */
const THROW_AIR_DRAG = 1.1;
/** Speed kept after hitting a screen edge. */
const THROW_BOUNCE_X = 0.55;
const THROW_BOUNCE_Y = 0.45;
/** Clamp on release speed, and the speed at which he is considered parked. */
const THROW_MAX_SPEED = 4200;
const THROW_SETTLE_SPEED = 40;
/** Only pointer samples this recent (ms) contribute to release velocity. */
const THROW_SAMPLE_WINDOW = 110;

/**
 * The frog's visible pixels inside one 32px sprite cell, covering both the
 * airborne and seated poses. Collisions use this instead of the window box:
 * the window is 340x420 with the sprite in a small patch at the bottom, so
 * bounding the window would bounce him off a wall of empty space.
 */
const SPRITE_BOX = { left: 7, top: 11, right: 25, bottom: 31 };
/** The sprite button sits `bottom-3` above the window's bottom edge. */
const FROG_BOTTOM_INSET = 12;

export type FrogMood = "idle" | "curious" | "croaking";

interface FeelingFrogProps {
  mood: FrogMood;
  intervention: FrogInterventionPayload | null;
  onGrounding: () => void;
  /** True while the frog is being carried, so the window stays interactive. */
  onCarry: (carrying: boolean) => void;
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
  onCarry,
}: FeelingFrogProps) {
  const hop = useAnimationControls();
  const lastId = useRef<string | null>(null);

  // True once a press turns into a real drag, i.e. the user is carrying the
  // frog. Held frogs pose in full extension instead of looping an animation.
  const [held, setHeld] = useState(false);
  const [heldPose, setHeldPose] = useState(0);
  // True while he is sailing through the air after being thrown.
  const [flying, setFlying] = useState(false);
  const [flightPose, setFlightPose] = useState(0);
  const [tilt, setTilt] = useState(0);

  // Animation frame id of the throw flight loop.
  const flightFrame = useRef<number | null>(null);

  const stopFlight = () => {
    if (flightFrame.current !== null) {
      cancelAnimationFrame(flightFrame.current);
      flightFrame.current = null;
    }
    setFlying(false);
    setTilt(0);
  };

  // Drop any in-flight animation when the component unmounts.
  useEffect(() => stopFlight, []);

  /** Throws the window along the sampled release velocity until it settles. */
  const throwFrog = async (recent: { t: number; x: number; y: number }[]) => {
    // A release with no recent motion is still a release: start from rest and
    // let gravity drop him to the desktop.
    let vx = 0;
    let vy = 0;
    if (recent.length >= 2) {
      const first = recent[0];
      const last = recent[recent.length - 1];
      const seconds = (last.t - first.t) / 1000;
      if (seconds > 0) {
        vx = (last.x - first.x) / seconds;
        vy = (last.y - first.y) / seconds;
        const speed = Math.hypot(vx, vy);
        if (speed > THROW_MAX_SPEED) {
          vx *= THROW_MAX_SPEED / speed;
          vy *= THROW_MAX_SPEED / speed;
        }
      }
    }

    const win = getCurrentWindow();
    let position, size, scale;
    try {
      [position, size, scale] = await Promise.all([
        win.outerPosition(),
        win.outerSize(),
        win.scaleFactor(),
      ]);
    } catch {
      return; // Window APIs unavailable (plain browser dev).
    }

    // Work area in physical px, so he lands on the desktop rather than under
    // the taskbar. `screen` reports CSS px, hence the scale factor.
    const area = screen as Screen & { availLeft?: number; availTop?: number };
    const areaLeft = (area.availLeft ?? 0) * scale;
    const areaTop = (area.availTop ?? 0) * scale;
    const areaRight = areaLeft + (area.availWidth || area.width) * scale;
    const areaBottom = areaTop + (area.availHeight || area.height) * scale;

    // Bounce the frog's visible body, not the transparent window box, so he
    // only stops at the actual screen edges.
    const cssWidth = size.width / scale;
    const cssHeight = size.height / scale;
    const buttonLeft = (cssWidth - RENDER_SIZE) / 2;
    const buttonTop = cssHeight - FROG_BOTTOM_INSET - RENDER_SIZE;
    const artLeft = (buttonLeft + RENDER_SIZE * (SPRITE_BOX.left / 32)) * scale;
    const artTop = (buttonTop + RENDER_SIZE * (SPRITE_BOX.top / 32)) * scale;
    const artRight = (buttonLeft + RENDER_SIZE * (SPRITE_BOX.right / 32)) * scale;
    const artBottom = (buttonTop + RENDER_SIZE * (SPRITE_BOX.bottom / 32)) * scale;

    const minX = areaLeft - artLeft;
    const minY = areaTop - artTop;
    const maxX = Math.max(minX, areaRight - artRight);
    const maxY = Math.max(minY, areaBottom - artBottom);
    let x = position.x;
    let y = position.y;

    setFlying(true);
    setHeld(false);
    let previous = performance.now();
    const startedAt = previous;

    const step = (now: number) => {
      const dt = Math.min((now - previous) / 1000, 0.05);
      previous = now;

      vy += THROW_GRAVITY * dt;
      const drag = Math.exp(-THROW_AIR_DRAG * dt);
      vx *= drag;
      vy *= drag;

      x += vx * dt;
      y += vy * dt;

      if (x < minX) {
        x = minX;
        vx = Math.abs(vx) * THROW_BOUNCE_X;
      } else if (x > maxX) {
        x = maxX;
        vx = -Math.abs(vx) * THROW_BOUNCE_X;
      }

      if (y < minY) {
        y = minY;
        vy = Math.abs(vy) * THROW_BOUNCE_Y;
      } else if (y >= maxY) {
        y = maxY;
        vy = -Math.abs(vy) * THROW_BOUNCE_Y;
        if (Math.abs(vy) < 320) vy = 0; // stop micro-bounces
      }

      const grounded = y >= maxY - 0.5 && vy === 0;
      if (grounded) vx *= Math.exp(-3.5 * dt); // sliding friction

      setTilt(Math.max(-22, Math.min(22, vx / 80)));
      void win
        .setPosition(new PhysicalPosition(Math.round(x), Math.round(y)))
        .catch(() => {});

      const settled = grounded && Math.abs(vx) < THROW_SETTLE_SPEED;
      if (settled || now - startedAt > 8000) {
        flightFrame.current = null;
        setFlying(false);
        setTilt(0);
        return;
      }
      flightFrame.current = requestAnimationFrame(step);
    };

    flightFrame.current = requestAnimationFrame(step);
  };

  // Dangle between the two extended poses while the frog is airborne.
  useEffect(() => {
    if (!held) {
      setHeldPose(0);
      return;
    }
    const id = setInterval(
      () => setHeldPose((current) => (current + 1) % HELD_FRAMES.length),
      1000 / HELD_FRAMES_PER_SECOND,
    );
    return () => clearInterval(id);
  }, [held]);

  // Slow alternation between the flying poses, each for its own hold time.
  useEffect(() => {
    if (!flying) {
      setFlightPose(0);
      return;
    }
    const id = setTimeout(
      () => setFlightPose((current) => (current + 1) % FLIGHT_FRAMES.length),
      FLIGHT_FRAMES[flightPose].hold,
    );
    return () => clearTimeout(id);
  }, [flying, flightPose]);

  // Drag state. The crown offset is fixed for the gesture and the window is
  // positioned from the live cursor every animation frame, rather than from
  // pointer deltas: a fast flick cannot outrun the window and leave him behind.
  const drag = useRef<{
    /** Crown position in physical px, relative to the window's top-left. */
    crownX: number;
    crownY: number;
    /** Cursor position at press, for the click/drag threshold. */
    startX: number;
    startY: number;
    scale: number;
    moved: boolean;
    /** Recent cursor samples (physical px) for release velocity. */
    samples: { t: number; x: number; y: number }[];
  } | null>(null);
  const followFrame = useRef<number | null>(null);
  const wasDragged = useRef(false);

  // Watches the real cursor position from the moment of the press. Movement is
  // detected here rather than from pointer events, so a fast flick that leaves
  // the window before the first `pointermove` still picks him up.
  const followCursor = async () => {
    followFrame.current = null;
    const state = drag.current;
    if (!state) return;
    try {
      const cursor = await cursorPosition();
      const now = performance.now();
      state.samples.push({ t: now, x: cursor.x, y: cursor.y });
      if (state.samples.length > 32) state.samples.shift();

      if (
        !state.moved &&
        Math.hypot(cursor.x - state.startX, cursor.y - state.startY) >=
          DRAG_THRESHOLD * state.scale
      ) {
        state.moved = true;
        wasDragged.current = true;
        setHeld(true);
        onCarry(true);
      }

      if (state.moved) {
        await getCurrentWindow().setPosition(
          new PhysicalPosition(
            Math.round(cursor.x - state.crownX),
            Math.round(cursor.y - state.crownY),
          ),
        );
      }
    } catch {
      // Window APIs unavailable (plain browser dev).
    }
    if (drag.current) {
      followFrame.current = requestAnimationFrame(() => void followCursor());
    }
  };

  const finishDrag = () => {
    const state = drag.current;
    if (!state) return;
    drag.current = null;
    if (followFrame.current !== null) {
      cancelAnimationFrame(followFrame.current);
      followFrame.current = null;
    }
    onCarry(false);
    setHeld(false);
    if (!state.moved) return; // A plain click is not a throw.

    // Only the tail of the movement counts, so pausing before letting go drops
    // him instead of flinging him.
    const cutoff = performance.now() - THROW_SAMPLE_WINDOW;
    void throwFrog(state.samples.filter((sample) => sample.t >= cutoff));
  };

  // The global left-button release ends a carry even when the cursor left the
  // window, so he can never be left hanging in the air.
  useEffect(() => {
    const unlisten = listen("pointer-release", () => finishDrag());
    return () => {
      void unlisten.then((off) => off());
    };
    // finishDrag only reads refs and calls stable setters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Cancel the follow loop if the component goes away mid-drag.
  useEffect(
    () => () => {
      if (followFrame.current !== null) {
        cancelAnimationFrame(followFrame.current);
        followFrame.current = null;
      }
    },
    [],
  );

  const handlePointerDown = async (event: React.PointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return;
    // Capture the element now: `currentTarget` is cleared once the handler
    // awaits, and the pointer never gets captured if this happens later.
    const button = event.currentTarget;
    // Catch him out of the air and clear any drag that never got a release.
    stopFlight();
    finishDrag();
    wasDragged.current = false;
    try {
      const rect = button.getBoundingClientRect();
      const scale = await getCurrentWindow().scaleFactor();
      drag.current = {
        crownX: (rect.left + rect.width * CROWN_X) * scale,
        crownY: (rect.top + rect.height * CROWN_Y) * scale,
        startX: event.screenX * scale,
        startY: event.screenY * scale,
        scale,
        moved: false,
        samples: [
          {
            t: performance.now(),
            x: event.screenX * scale,
            y: event.screenY * scale,
          },
        ],
      };
      button.setPointerCapture(event.pointerId);
      void followCursor();
    } catch {
      // Window APIs unavailable (plain browser dev): leave dragging disabled.
    }
  };

  const endDrag = (event: React.PointerEvent<HTMLButtonElement>) => {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    finishDrag();
  };

  const handleClick = () => {
    const dragged = wasDragged.current;
    wasDragged.current = false;
    finishDrag();
    if (dragged) return; // A drag must not trigger the croak-along.
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

  const croaking = mood === "croaking";
  const curious = mood === "curious";
  // Croaking along runs slow and deliberate; curiosity is a quicker riff.
  const animation: AnimationName = croaking || curious ? "croak" : "idle";
  const framesPerSecond = croaking ? 2.5 : curious ? 8 : 6;
  const frame = useSpriteFrame(animation, framesPerSecond);

  // Carried and airborne poses come from fixed frames rather than the mood
  // loop: dangling while held, wide-eyed and stretched while thrown.
  const pose = flying
    ? FLIGHT_FRAMES[flightPose]
    : held
      ? {
          column: ANIMATIONS.jump.column + HELD_FRAMES[heldPose],
          row: ANIMATIONS.jump.row,
        }
      : {
          column: ANIMATIONS[animation].column + frame,
          row: ANIMATIONS[animation].row,
        };

  const sheetColumn = pose.column;
  const sheetRow = pose.row;

  return (
    <div className="relative h-full w-full">
      <div className="absolute inset-x-0 bottom-[calc(0.75rem+192px)] flex justify-center">
        <AnimatePresence>
          {intervention && !croaking && (
            <motion.div
              key={intervention.id}
              data-interactive
              initial={{ opacity: 0, y: 8, scale: 0.92 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 6, scale: 0.94 }}
              transition={{ type: "spring", stiffness: 320, damping: 24 }}
              className="pointer-events-auto relative w-[260px] overflow-hidden rounded-2xl border border-emerald-900/10 bg-emerald-50/95 px-3.5 pt-2.5 pb-3.5 text-[12px] leading-snug text-emerald-950 shadow-lg shadow-emerald-950/10 backdrop-blur"
            >
              <p className="font-semibold">{intervention.message}</p>
              <p className="mt-1 text-[11px] text-emerald-700">
                {ACTION_LABEL[intervention.suggestedAction]}
              </p>
              {/* Timer bar: drains linearly until the bubble auto-dismisses. */}
              <span
                aria-hidden
                className="absolute inset-x-0 bottom-0 h-[3px] bg-emerald-900/10"
              >
                <motion.span
                  key={`${intervention.id}-timer`}
                  className="block h-full origin-left bg-emerald-500/70"
                  initial={{ scaleX: 1 }}
                  animate={{ scaleX: 0 }}
                  transition={{
                    duration: INTERVENTION_TTL_MS / 1000,
                    ease: "linear",
                  }}
                />
              </span>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      <div className="absolute inset-x-0 bottom-3 flex justify-center">
      <motion.button
        type="button"
        data-interactive
        aria-label="Drag to reposition the frog, click to croak along"
        onClick={handleClick}
        onPointerDown={handlePointerDown}
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
          className="absolute bottom-1 left-1/2 h-3 w-3/4 -translate-x-1/2 rounded-full bg-emerald-950/25 blur-[6px] transition-opacity duration-200"
          style={{ opacity: flying ? 0 : 1 }}
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
            // Lean into the throw, then straighten up once he lands.
            transform: tilt === 0 ? undefined : `rotate(${tilt}deg)`,
          }}
        />
      </motion.button>
      </div>
    </div>
  );
}
