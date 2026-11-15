import { listen } from "@tauri-apps/api/event";
import { cursorPosition, getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import type { BreathPhase } from "./breathing";
import { applySoundSettings } from "./croak";
import { loadSettings, watchSettings } from "./settings";
import { BreatheAlong } from "./components/BreatheAlong";
import { ChatWithFrog } from "./components/ChatWithFrog";
import { FeelingFrog, type FrogMood } from "./components/FeelingFrog";
import { MindfulPause } from "./components/MindfulPause";
import { SelectionOffer } from "./components/SelectionOffer";
import { TaskIntention } from "./components/TaskIntention";
import { VisualGrounding } from "./components/VisualGrounding";
import {
  INTERVENTION_TTL_MS,
  type FrogInterventionPayload,
  type SelectionReadyPayload,
} from "./types";

/** Guided activities the frog can lead; each maps to one `suggestedAction`. */
type OpenActivity =
  | "belly_breathe"
  | "visual_grounding"
  | "task_intent"
  | "mindful_pause"
  | "chat";

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
  const [selectionOffer, setSelectionOffer] =
    useState<SelectionReadyPayload | null>(null);
  const [needsAccessibility, setNeedsAccessibility] = useState(false);
  const [activity, setActivity] = useState<OpenActivity | null>(null);
  const [carrying, setCarrying] = useState(false);
  const [breathPhase, setBreathPhase] = useState<BreathPhase | null>(null);

  // Keep the whole window interactive while he is being carried or the guide is
  // up; otherwise the hitbox tick can make it click-through mid-drag and drop
  // the pointer capture.
  useClickThroughHitbox(
    activity !== null || carrying || selectionOffer !== null || needsAccessibility,
  );

  useEffect(() => {
    const unlisten = listen<FrogInterventionPayload>(
      "trigger-intervention",
      (event) => setIntervention(event.payload),
    );
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  // Long highlights arrive from the backend watcher; the frog offers a summary.
  useEffect(() => {
    const offer = listen<SelectionReadyPayload>("selection-ready", (event) =>
      setSelectionOffer(event.payload),
    );
    const hint = listen("selection-unavailable", () =>
      setNeedsAccessibility(true),
    );
    return () => {
      void offer.then((off) => off());
      void hint.then((off) => off());
    };
  }, []);

  // Sound preferences live in the backend (shared with the tray menu); mirror
  // them into the croak synth and follow live changes from the control panel.
  useEffect(() => {
    void loadSettings().then(applySoundSettings).catch(() => {});
    const unlisten = watchSettings(applySoundSettings);
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  // Auto-dismiss the bubble so it never lingers on screen, including demos from
  // the Testing tab.
  useEffect(() => {
    if (!intervention) return;
    const timer = setTimeout(() => setIntervention(null), INTERVENTION_TTL_MS);
    return () => clearTimeout(timer);
  }, [intervention]);

  const mood: FrogMood = activity === "belly_breathe"
    ? "croaking"
    : activity
      ? "curious"
      : intervention
        ? intervention.trigger === "doomscroll"
          ? "grumpy"
          : "curious"
        : "idle";

  const closeActivity = () => {
    setActivity(null);
    setBreathPhase(null);
    setIntervention(null);
  };

  // Clicking the frog opens the guide matching the nudge's suggested action;
  // with no nudge up it just opens a chat.
  const openGrounding = () => {
    const action = intervention?.suggestedAction;
    if (!action) {
      setActivity("chat");
    } else {
      setActivity(action === "belly_breathe" ? "belly_breathe" : action);
    }
    setIntervention(null);
  };

  return (
    <div className="flex h-full w-full items-end justify-center bg-transparent">
      <FeelingFrog
        mood={mood}
        intervention={intervention}
        onGrounding={openGrounding}
        onCarry={setCarrying}
        breathPhase={activity === "belly_breathe" ? breathPhase : null}
      />
      <BreatheAlong
        open={activity === "belly_breathe"}
        onClose={closeActivity}
        onPhase={setBreathPhase}
      />
      <VisualGrounding
        open={activity === "visual_grounding"}
        onClose={closeActivity}
      />
      <TaskIntention
        open={activity === "task_intent"}
        onClose={closeActivity}
      />
      <MindfulPause
        open={activity === "mindful_pause"}
        onClose={closeActivity}
      />
      <ChatWithFrog open={activity === "chat"} onClose={closeActivity} />
      <SelectionOffer
        offer={selectionOffer}
        hint={needsAccessibility}
        onDismiss={() => setSelectionOffer(null)}
        onDismissHint={() => setNeedsAccessibility(false)}
      />
    </div>
  );
}
