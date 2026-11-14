export type BurnoutTrigger = 'frantic_typing' | 'backspace_loop' | 'tab_thrashing' | 'idle_freeze' | 'cursor_hesitation' | 'cursor_oscillation' | 'doomscroll';

/** How long an intervention bubble stays up before auto-dismissing. */
export const INTERVENTION_TTL_MS = 10_000;

export interface FrogInterventionPayload {
    id: string;
    trigger: BurnoutTrigger;
    severity: 'low' | 'medium' | 'high';
    message: string;           // e.g. "*ribbit* You've hit backspace 40 times in 10 seconds. Let's pause on the lily pad."
    suggestedAction: 'belly_breathe' | 'visual_grounding' | 'task_intent' | 'mindful_pause';
    activeApp?: string;        // e.g. "Google Chrome"
}

/** A long passage the user highlighted; the frog offers to summarize it. */
export interface SelectionReadyPayload {
    text: string;
    words: number;
}
