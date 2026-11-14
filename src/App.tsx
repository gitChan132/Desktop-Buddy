import { listen } from "@tauri-apps/api/event";
import { cursorPosition, getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { BreatheAlong } from "./components/BreatheAlong";
import { FeelingFrog, type FrogMood } from "./components/FeelingFrog";
import { INTERVENTION_TTL_MS, type FrogInterventionPayload } from "./types";

// Region (logical px, anchored bottom-center) where the frog is clickable.
// Everything else in the transparent window stays click-through.
const HITBOX_WIDTH = 250;
const HITBOX_HEIGHT = 330;

function useClickThroughHitbox(forceInteractive: boolean) {
  useEffect(() => {
    const win = getCurrentWindow();
    let stopped = false;
    let inFlight = false;

    const tick = async () => {
      if (stopped || inFlight) return;
      inFlight = true;
      try {
        const [cursor, outer, inner, scale] = await Promise.all([
          cursorPosition(),
          win.outerPosition(),
          win.innerSize(),
          win.scaleFactor(),
        ]);
        const x = (cursor.x - outer.x) / scale;
        const y = (cursor.y - outer.y) / scale;
        const width = inner.width / scale;
        const height = inner.height / scale;
        const overFrog =
          x >= width - HITBOX_WIDTH &&
          y >= height - HITBOX_HEIGHT &&
          x <= width &&
          y <= height;
        await win.setIgnoreCursorEvents(!(overFrog || forceInteractive));
      } catch {
        // Window APIs unavailable (e.g. plain browser dev): stay interactive.
      } finally {
        inFlight = false;
      }
    };

    void tick();
    const id = setInterval(tick, 120);
    return () => {
      stopped = true;
      clearInterval(id);
      void win.setIgnoreCursorEvents(false);
    };
  }, [forceInteractive]);
}

export default function App() {
  const [intervention, setIntervention] =
    useState<FrogInterventionPayload | null>(null);
  const [breathingOpen, setBreathingOpen] = useState(false);
  const [carrying, setCarrying] = useState(false);

  // Keep the whole window interactive while he is being carried or the guide is
  // up; otherwise the hitbox tick can make it click-through mid-drag and drop
  // the pointer capture.
  useClickThroughHitbox(breathingOpen || carrying);

  useEffect(() => {
    const unlisten = listen<FrogInterventionPayload>(
      "trigger-intervention",
      (event) => setIntervention(event.payload),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  // Auto-dismiss the bubble so it never lingers on screen.
  useEffect(() => {
    if (!intervention) return;
    const timer = setTimeout(() => setIntervention(null), INTERVENTION_TTL_MS);
    return () => clearTimeout(timer);
  }, [intervention]);

  const mood: FrogMood = breathingOpen
    ? "croaking"
    : intervention
      ? "curious"
      : "idle";

  const closeBreathing = () => {
    setBreathingOpen(false);
    setIntervention(null);
  };

  return (
    <div className="flex h-full w-full items-end justify-center bg-transparent">
      <FeelingFrog
        mood={mood}
        intervention={intervention}
        onGrounding={() => setBreathingOpen(true)}
        onCarry={setCarrying}
      />
      <BreatheAlong open={breathingOpen} onClose={closeBreathing} />
    </div>
  );
}
