use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Runtime preferences shared by the control-panel window, the tray menu and
/// the evaluation loop. Session-only — defaults return on every launch.
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub nudges_paused: bool,
    pub sound_enabled: bool,
    pub volume: f32,
}

static NUDGES_PAUSED: AtomicBool = AtomicBool::new(false);
static SOUND_ENABLED: AtomicBool = AtomicBool::new(true);
/// Croak loudness in `0.0..=1.0`, stored as raw f32 bits to stay lock-free.
static VOLUME: AtomicU32 = AtomicU32::new(0.5f32.to_bits());

pub fn settings() -> Settings {
    Settings {
        nudges_paused: NUDGES_PAUSED.load(Ordering::Relaxed),
        sound_enabled: SOUND_ENABLED.load(Ordering::Relaxed),
        volume: f32::from_bits(VOLUME.load(Ordering::Relaxed)),
    }
}

pub fn nudges_paused() -> bool {
    NUDGES_PAUSED.load(Ordering::Relaxed)
}

pub fn sound_enabled() -> bool {
    SOUND_ENABLED.load(Ordering::Relaxed)
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
