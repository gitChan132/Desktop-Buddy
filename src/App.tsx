import { listen } from "@tauri-apps/api/event";
import { cursorPosition, getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { FeelingFrog, type FrogMood } from "./components/FeelingFrog";
import { LilyPadGrounding } from "./components/LilyPadGrounding";
import type { FrogInterventionPayload } from "./types";

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
  const [groundingOpen, setGroundingOpen] = useState(false);

  useClickThroughHitbox(groundingOpen);

  useEffect(() => {
    const unlisten = listen<FrogInterventionPayload>(
      "trigger-intervention",
      (event) => setIntervention(event.payload),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  const mood: FrogMood = groundingOpen ? "breathing" : intervention ? "curious" : "idle";

  const closeGrounding = () => {
    setGroundingOpen(false);
    setIntervention(null);
  };

  return (
    <div className="flex h-full w-full items-end justify-center bg-transparent">
      <FeelingFrog
        mood={mood}
        intervention={intervention}
        onGrounding={() => setGroundingOpen(true)}
      />
      <LilyPadGrounding
        open={groundingOpen}
        suggestedAction={intervention?.suggestedAction ?? "belly_breathe"}
        onClose={closeGrounding}
      />
    </div>
  );
}
