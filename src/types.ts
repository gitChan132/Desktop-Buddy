export type BurnoutTrigger = 'frantic_typing' | 'backspace_loop' | 'tab_thrashing' | 'idle_freeze';

export interface FrogInterventionPayload {
    id: string;
    trigger: BurnoutTrigger;
    severity: 'low' | 'medium' | 'high';
    message: string;           // e.g. "*ribbit* You've hit backspace 40 times in 10 seconds. Let's pause on the lily pad."
    suggestedAction: 'belly_breathe' | 'lily_pad_step' | 'pond_stretch';
    activeApp?: string;        // e.g. "Google Chrome"
}
