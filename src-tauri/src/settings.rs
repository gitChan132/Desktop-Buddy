use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::time::Duration;

/// Trigger ids, in bit order. Keep in sync with `BurnoutTrigger` in types.ts.
const TRIGGERS: [&str; 7] = [
    "frantic_typing",
    "backspace_loop",
    "tab_thrashing",
    "idle_freeze",
    "cursor_hesitation",
    "cursor_oscillation",
    "doomscroll",
];

/// Activity ids, in bit order. Keep in sync with `suggestedAction` in types.ts.
const ACTIVITIES: [&str; 4] = [
    "belly_breathe",
    "visual_grounding",
    "task_intent",
    "mindful_pause",
];

/// Mode ids, in bit order. Keep in sync with `PondMode` in settings.ts.
const MODES: [&str; 3] = ["focus", "chill", "fun"];

/// Frog palette ids, in cycle order. Keep in sync with `FrogColor` in
/// sprites.ts.
const FROG_COLORS: [&str; 4] = ["green", "blue", "pink", "amber"];

/// Which spritesheet the frog wears.
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FrogColor {
    Green,
    Blue,
    Pink,
    Amber,
}

/// Mood of the pond. Focus nudges sooner, Chill leaves you alone, Fun keeps it
/// light and quick.
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Focus,
    Chill,
    Fun,
}

/// Which detectors are allowed to fire.
static TRIGGER_BITS: AtomicU8 = AtomicU8::new((1u8 << 7) - 1);
/// Which activities the engine may suggest.
static ACTIVITY_BITS: AtomicU8 = AtomicU8::new((1u8 << 4) - 1);

fn index_of(list: &[&str], id: &str) -> Option<u8> {
    list.iter().position(|name| *name == id).map(|index| index as u8)
}

fn is_enabled(bits: &AtomicU8, list: &[&str], id: &str) -> bool {
    index_of(list, id)
        .map(|index| bits.load(Ordering::Relaxed) & (1 << index) != 0)
        .unwrap_or(false)
}

fn set_enabled(bits: &AtomicU8, list: &[&str], id: &str, enabled: bool) {
    if let Some(index) = index_of(list, id) {
        let mask = 1u8 << index;
        let mut current = bits.load(Ordering::Relaxed);
        if enabled {
            current |= mask;
        } else {
            current &= !mask;
        }
        bits.store(current, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerSettings {
    pub frantic_typing: bool,
    pub backspace_loop: bool,
    pub tab_thrashing: bool,
    pub idle_freeze: bool,
    pub cursor_hesitation: bool,
    pub cursor_oscillation: bool,
    pub doomscroll: bool,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySettings {
    pub belly_breathe: bool,
    pub visual_grounding: bool,
    pub task_intent: bool,
    pub mindful_pause: bool,
}

/// Runtime preferences shared by the pond, the popover, the tray menu and the
/// evaluation loop. Session-only — defaults return on every launch.
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub nudges_paused: bool,
    pub sound_enabled: bool,
    pub volume: f32,
    pub mode: Mode,
    pub frog_color: FrogColor,
    pub selection_summaries: bool,
    pub triggers: TriggerSettings,
    pub activities: ActivitySettings,
}

static NUDGES_PAUSED: AtomicBool = AtomicBool::new(false);
/// Offers to summarize long highlights.
static SELECTION_SUMMARIES: AtomicBool = AtomicBool::new(true);
/// Index into `MODES`; 0 is Focus.
static MODE: AtomicU8 = AtomicU8::new(0);
/// Index into `FROG_COLORS`; 0 is Green.
static FROG_COLOR: AtomicU8 = AtomicU8::new(0);
static SOUND_ENABLED: AtomicBool = AtomicBool::new(true);
/// Croak loudness in `0.0..=1.0`, stored as raw f32 bits to stay lock-free.
static VOLUME: AtomicU32 = AtomicU32::new(0.5f32.to_bits());

pub fn settings() -> Settings {
    Settings {
        nudges_paused: NUDGES_PAUSED.load(Ordering::Relaxed),
        sound_enabled: SOUND_ENABLED.load(Ordering::Relaxed),
        volume: f32::from_bits(VOLUME.load(Ordering::Relaxed)),
        mode: mode(),
        frog_color: frog_color(),
        selection_summaries: selection_summaries(),
        triggers: TriggerSettings {
            frantic_typing: trigger_enabled("frantic_typing"),
            backspace_loop: trigger_enabled("backspace_loop"),
            tab_thrashing: trigger_enabled("tab_thrashing"),
            idle_freeze: trigger_enabled("idle_freeze"),
            cursor_hesitation: trigger_enabled("cursor_hesitation"),
            cursor_oscillation: trigger_enabled("cursor_oscillation"),
            doomscroll: trigger_enabled("doomscroll"),
        },
        activities: ActivitySettings {
            belly_breathe: activity_enabled("belly_breathe"),
            visual_grounding: activity_enabled("visual_grounding"),
            task_intent: activity_enabled("task_intent"),
            mindful_pause: activity_enabled("mindful_pause"),
        },
    }
}

pub fn nudges_paused() -> bool {
    NUDGES_PAUSED.load(Ordering::Relaxed)
}

pub fn sound_enabled() -> bool {
    SOUND_ENABLED.load(Ordering::Relaxed)
}

pub fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 => Mode::Chill,
        2 => Mode::Fun,
        _ => Mode::Focus,
    }
}

/// Unknown names are ignored so a stale panel can't wedge the mode.
pub fn set_mode(name: &str) {
    if let Some(index) = index_of(&MODES, name) {
        MODE.store(index, Ordering::Relaxed);
    }
}

pub fn frog_color() -> FrogColor {
    match FROG_COLOR.load(Ordering::Relaxed) {
        1 => FrogColor::Blue,
        2 => FrogColor::Pink,
        3 => FrogColor::Amber,
        _ => FrogColor::Green,
    }
}

/// Unknown names are ignored so a stale pond can't wedge the palette.
pub fn set_frog_color(name: &str) {
    if let Some(index) = index_of(&FROG_COLORS, name) {
        FROG_COLOR.store(index, Ordering::Relaxed);
    }
}

/// How often the frog may interrupt. Focus nips problems early, Chill stays
/// mostly out of the way, Fun is brisk but light.
pub fn intervention_cooldown() -> Duration {
    match mode() {
        Mode::Focus => Duration::from_secs(25),
        Mode::Chill => Duration::from_secs(120),
        Mode::Fun => Duration::from_secs(30),
    }
}

pub fn selection_summaries() -> bool {
    SELECTION_SUMMARIES.load(Ordering::Relaxed)
}

pub fn set_selection_summaries(enabled: bool) {
    SELECTION_SUMMARIES.store(enabled, Ordering::Relaxed);
}

pub fn set_nudges_paused(paused: bool) {
    NUDGES_PAUSED.store(paused, Ordering::Relaxed);
}

pub fn set_sound_enabled(enabled: bool) {
    SOUND_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn set_volume(volume: f32) {
    VOLUME.store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
}

pub fn trigger_enabled(id: &str) -> bool {
    is_enabled(&TRIGGER_BITS, &TRIGGERS, id)
}

/// What the detectors may act on: Chill mutes doomscroll so scrolling around
/// doesn't get judged, whatever the toggle says.
pub fn detector_enabled(id: &str) -> bool {
    trigger_enabled(id) && !(id == "doomscroll" && mode() == Mode::Chill)
}

pub fn set_trigger_enabled(id: &str, enabled: bool) {
    set_enabled(&TRIGGER_BITS, &TRIGGERS, id, enabled);
}

pub fn activity_enabled(id: &str) -> bool {
    is_enabled(&ACTIVITY_BITS, &ACTIVITIES, id)
}

pub fn set_activity_enabled(id: &str, enabled: bool) {
    set_enabled(&ACTIVITY_BITS, &ACTIVITIES, id, enabled);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_and_activity_bits_round_trip() {
        // Restore the defaults so other tests see them enabled.
        let before = (
            TRIGGER_BITS.load(Ordering::Relaxed),
            ACTIVITY_BITS.load(Ordering::Relaxed),
        );
        assert!(trigger_enabled("doomscroll"));
        set_trigger_enabled("doomscroll", false);
        assert!(!trigger_enabled("doomscroll"));
        assert!(trigger_enabled("tab_thrashing"));
        set_trigger_enabled("doomscroll", true);
        assert!(trigger_enabled("doomscroll"));

        set_activity_enabled("task_intent", false);
        assert!(!activity_enabled("task_intent"));
        assert!(activity_enabled("belly_breathe"));
        set_activity_enabled("task_intent", true);

        // Unknown ids stay disabled and are ignored on write.
        assert!(!trigger_enabled("nope"));
        set_trigger_enabled("nope", true);
        assert!(!trigger_enabled("nope"));

        TRIGGER_BITS.store(before.0, Ordering::Relaxed);
        ACTIVITY_BITS.store(before.1, Ordering::Relaxed);
    }
}
