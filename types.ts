export type BurnoutTrigger = 'frantic_typing' | 'backspace_loop' | 'tab_thrashing' | 'idle_freeze';

export interface InterventionPayload {
    id: string;
    trigger: BurnoutTrigger;
    severity: 'low' | 'medium' | 'high';
    message: string;           // e.g. "You've hit backspace 40 times in 10 seconds. Let's pause."
    suggestedAction: 'breathe' | 'micro_step' | 'stretch';
    activeApp?: string;        // e.g. "Google Chrome"
}
