//! Privacy-first input telemetry.
//!
//! Only timing metadata is recorded: keypress timestamps, `Backspace` counts, and
//! pointer velocity samples, all inside strictly bounded rolling 30-second buffers.
//! No key contents, text, or clipboard data ever leave this module. Pointer
//! coordinates are positional metadata used only to keep the frog's hitbox in
//! sync; they are throttled to 20 Hz before emission and never persisted.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rdev::{listen, Button, Event, EventType, Key};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Rolling window for every telemetry buffer.
const WINDOW: Duration = Duration::from_secs(30);
/// Hard cap on buffered pointer speed samples.
const POINTER_SAMPLES: usize = 120;
/// A backspace run ends after this much silence.
const BACKSPACE_STREAK_TIMEOUT: Duration = Duration::from_secs(5);
/// How often the normalized pond ripple stress score is recomputed.
const STRESS_INTERVAL: Duration = Duration::from_secs(5);
/// Emit cursor coordinates at most once per 20 Hz frame.
const CURSOR_THROTTLE: Duration = Duration::from_millis(50);
/// Event consumed by the frontend hitbox check.
const CURSOR_EVENT: &str = "cursor-pos";
/// Event carrying the normalized pond ripple stress score.
const STRESS_EVENT: &str = "pond-stress";
/// Fired when the primary mouse button is released anywhere on the desktop, so
/// the frog always lets go of a drag even if the cursor left its window.
const RELEASE_EVENT: &str = "pointer-release";

// Per-window values that saturate their component of the stress score.
const MAX_KEYS_PER_WINDOW: f32 = 180.0; // 6 keys/s sustained
const MAX_BACKSPACES_PER_WINDOW: f32 = 24.0; // 0.8 backspaces/s sustained
const MAX_MEAN_SPEED: f32 = 3000.0; // px/s
const MAX_JITTER: f32 = 2000.0; // px/s std dev
/// A click after this long of cursor stillness counts as a hesitant click.
const HESITATION_MIN_SECS: f32 = 2.0;
/// Mean stillness-before-click that saturates the hesitation component.
const MAX_HESITATION_SECS: f32 = 3.0;
/// A horizontal step must exceed this many px to count toward a reversal.
const REVERSAL_MIN_STEP: f64 = 6.0;
/// Direction reversals are judged within this shorter window.
const OSCILLATION_WINDOW: Duration = Duration::from_secs(10);
/// Reversals that saturate the oscillation component of the stress score.
const MAX_REVERSALS: f32 = 8.0;

#[derive(Default)]
struct TypingTelemetry {
    keypresses: VecDeque<Instant>,
    backspaces: VecDeque<Instant>,
    last_keypress: Option<Instant>,
    /// Consecutive backspaces; reset by any other key or a pause in typing.
    backspace_streak: u32,
    last_backspace: Option<Instant>,
}

#[derive(Default)]
struct PointerTelemetry {
    last_position: Option<(f64, f64, Instant)>,
    /// Instantaneous pointer speeds in px/s, oldest first.
    speeds: VecDeque<(Instant, f32)>,
    last_move: Option<Instant>,
    last_emit: Option<Instant>,
    /// Seconds the cursor stayed still before each click, in the rolling window.
    hesitations: VecDeque<(Instant, f32)>,
    /// Horizontal direction of the last significant step (-1, 0, +1).
    last_move_sign: i8,
    /// Timestamps of significant horizontal direction reversals.
    reversals: VecDeque<Instant>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TypingSnapshot {
    pub keys_in_window: usize,
    pub backspaces_in_window: usize,
    /// Current run of consecutive backspaces (0 if the run ended).
    pub backspace_streak: u32,
}

/// Throttled pointer sample emitted as the `cursor-pos` event.
/// Coordinates use the OS pointer space reported by `rdev` (logical points on
/// macOS), so the frontend must apply the window scale factor itself.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct CursorSample {
    pub x: f64,
    pub y: f64,
}

/// Normalized (`0.0..=1.0`) pond ripple stress score emitted as `pond-stress`.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct PondStress {
    pub score: f32,
}

/// Pluggable scoring strategy for the pond ripple stress score.
trait StressModel: Send + Sync {
    fn score(&self, typing: &TypingTelemetry, pointer: &PointerTelemetry) -> f32;
}

/// Default strategy: weighted blend of typing, backspace, velocity, and jitter.
struct WeightedStress;

impl StressModel for WeightedStress {
    fn score(&self, typing: &TypingTelemetry, pointer: &PointerTelemetry) -> f32 {
        let key_pressure = (typing.keypresses.len() as f32 / MAX_KEYS_PER_WINDOW).clamp(0.0, 1.0);
        let backspace_pressure =
            (typing.backspaces.len() as f32 / MAX_BACKSPACES_PER_WINDOW).clamp(0.0, 1.0);
        let (mean_speed, jitter) = speed_stats(&pointer.speeds);
        let velocity_pressure = (mean_speed / MAX_MEAN_SPEED).clamp(0.0, 1.0);
        let jitter_pressure = (jitter / MAX_JITTER).clamp(0.0, 1.0);
        let (_, mean_hesitation) = hesitation_stats(&pointer.hesitations);
        let hesitation_pressure = (mean_hesitation / MAX_HESITATION_SECS).clamp(0.0, 1.0);
        let oscillation_pressure = (pointer.reversals.len() as f32 / MAX_REVERSALS).clamp(0.0, 1.0);

        (key_pressure * 0.25
            + backspace_pressure * 0.15
            + velocity_pressure * 0.15
            + jitter_pressure * 0.10
            + hesitation_pressure * 0.15
            + oscillation_pressure * 0.20)
            .clamp(0.0, 1.0)
    }
}

fn stress_model() -> &'static dyn StressModel {
    static MODEL: WeightedStress = WeightedStress;
    &MODEL
}

fn typing() -> &'static Mutex<TypingTelemetry> {
    static TYPING: OnceLock<Mutex<TypingTelemetry>> = OnceLock::new();
    TYPING.get_or_init(|| Mutex::new(TypingTelemetry::default()))
}

fn pointer() -> &'static Mutex<PointerTelemetry> {
    static POINTER: OnceLock<Mutex<PointerTelemetry>> = OnceLock::new();
    POINTER.get_or_init(|| Mutex::new(PointerTelemetry::default()))
}

/// Rolling window for wheel activity.
const SCROLL_WINDOW: Duration = Duration::from_secs(90);

/// Only wheel timing is kept: no page content, no position, no target app.
#[derive(Default)]
struct ScrollTelemetry {
    /// One entry per wheel event that moved the page.
    events: VecDeque<Instant>,
}

/// Wheel activity seen in the rolling window.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollSnapshot {
    pub scrolls_in_window: usize,
}

fn scroll() -> &'static Mutex<ScrollTelemetry> {
    static SCROLL: OnceLock<Mutex<ScrollTelemetry>> = OnceLock::new();
    SCROLL.get_or_init(|| Mutex::new(ScrollTelemetry::default()))
}

fn prune_scrolls(state: &mut ScrollTelemetry, now: Instant) {
    while state
        .events
        .front()
        .is_some_and(|stamp| now.duration_since(*stamp) > SCROLL_WINDOW)
    {
        state.events.pop_front();
    }
}

fn note_scroll(now: Instant) {
    let mut state = scroll().lock().expect("scroll telemetry lock poisoned");
    state.events.push_back(now);
    prune_scrolls(&mut state, now);
}

/// Wheel events in the last [`SCROLL_WINDOW`], used by the doomscroll detector.
pub fn scroll_snapshot() -> ScrollSnapshot {
    let now = Instant::now();
    let mut state = scroll().lock().expect("scroll telemetry lock poisoned");
    prune_scrolls(&mut state, now);
    ScrollSnapshot {
        scrolls_in_window: state.events.len(),
    }
}

/// Latest score in `f32` bits, readable without taking a lock.
fn latest_stress() -> &'static AtomicU32 {
    static STRESS: AtomicU32 = AtomicU32::new(0);
    &STRESS
}

/// Launch anchor, so "no input yet" can still age into idleness.
fn startup() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

fn prune(queue: &mut VecDeque<Instant>, now: Instant) {
    while queue
        .front()
        .is_some_and(|stamp| now.duration_since(*stamp) > WINDOW)
    {
        queue.pop_front();
    }
}

fn prune_speeds(queue: &mut VecDeque<(Instant, f32)>, now: Instant) {
    while queue
        .front()
        .is_some_and(|(stamp, _)| now.duration_since(*stamp) > WINDOW)
    {
        queue.pop_front();
    }
    while queue.len() > POINTER_SAMPLES {
        queue.pop_front();
    }
}

fn note_keypress(state: &mut TypingTelemetry, key: Key, now: Instant) {
    prune(&mut state.keypresses, now);
    prune(&mut state.backspaces, now);

    state.keypresses.push_back(now);
    state.last_keypress = Some(now);

    if matches!(key, Key::Backspace) {
        state.backspaces.push_back(now);
        let run_is_live = state
            .last_backspace
            .is_some_and(|previous| now.duration_since(previous) <= BACKSPACE_STREAK_TIMEOUT);
        state.backspace_streak = if run_is_live {
            state.backspace_streak + 1
        } else {
            1
        };
        state.last_backspace = Some(now);
    } else {
        state.backspace_streak = 0;
    }
}

/// Returns `(hesitant_clicks, mean stillness secs)` where a hesitant click is
/// one preceded by at least [`HESITATION_MIN_SECS`] of no cursor movement.
fn hesitation_stats(hesitations: &VecDeque<(Instant, f32)>) -> (usize, f32) {
    if hesitations.is_empty() {
        return (0, 0.0);
    }
    let mean = hesitations.iter().map(|(_, dwell)| *dwell).sum::<f32>() / hesitations.len() as f32;
    let hesitant_clicks = hesitations
        .iter()
        .filter(|(_, dwell)| *dwell >= HESITATION_MIN_SECS)
        .count();
    (hesitant_clicks, mean)
}

fn prune_hesitations(queue: &mut VecDeque<(Instant, f32)>, now: Instant) {
    while queue
        .front()
        .is_some_and(|(stamp, _)| now.duration_since(*stamp) > WINDOW)
    {
        queue.pop_front();
    }
}

/// Records how long the cursor was still before this click. Measured from the
/// last movement (or previous click), then that anchor is reset to this click so
/// repeated clicks at one spot don't accumulate ever-growing dwell times.
fn note_click(state: &mut PointerTelemetry, now: Instant) {
    prune_hesitations(&mut state.hesitations, now);
    if let Some((x, y, at)) = state.last_position {
        let dwell = now.duration_since(at).as_secs_f32();
        state.hesitations.push_back((now, dwell));
        while state.hesitations.len() > POINTER_SAMPLES {
            state.hesitations.pop_front();
        }
        state.last_position = Some((x, y, now));
    }
    // A click is input too, so it should reset idleness.
    state.last_move = Some(now);
}

fn prune_reversals(queue: &mut VecDeque<Instant>, now: Instant) {
    while queue
        .front()
        .is_some_and(|stamp| now.duration_since(*stamp) > OSCILLATION_WINDOW)
    {
        queue.pop_front();
    }
}

/// Counts a reversal when a significant horizontal step flips direction, which
/// is the fingerprint of the cursor moving back and forth between locations.
fn note_move_direction(state: &mut PointerTelemetry, last_x: f64, x: f64, now: Instant) {
    let dx = x - last_x;
    if dx.abs() < REVERSAL_MIN_STEP {
        return;
    }
    let sign = if dx > 0.0 { 1 } else { -1 };
    if state.last_move_sign != 0 && sign != state.last_move_sign {
        prune_reversals(&mut state.reversals, now);
        state.reversals.push_back(now);
        while state.reversals.len() > POINTER_SAMPLES {
            state.reversals.pop_front();
        }
    }
    state.last_move_sign = sign;
}

/// Number of significant back-and-forth reversals in the oscillation window.
pub fn direction_reversals() -> usize {
    let now = Instant::now();
    let mut state = pointer().lock().expect("pointer telemetry lock poisoned");
    prune_reversals(&mut state.reversals, now);
    state.reversals.len()
}

/// Rolling hesitation picture exposed to the window snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct HesitationSnapshot {
    /// Clicks in the window preceded by at least [`HESITATION_MIN_SECS`] of stillness.
    pub hesitant_clicks: usize,
    /// Mean seconds of stillness before a click across the window.
    pub mean_hesitation_secs: f32,
}

/// Current hesitation stats over the rolling window.
pub fn hesitation_snapshot() -> HesitationSnapshot {
    let now = Instant::now();
    let mut state = pointer().lock().expect("pointer telemetry lock poisoned");
    prune_hesitations(&mut state.hesitations, now);
    let (hesitant_clicks, mean_hesitation_secs) = hesitation_stats(&state.hesitations);
    HesitationSnapshot {
        hesitant_clicks,
        mean_hesitation_secs,
    }
}

/// Returns `(mean px/s, std dev px/s)` for the sampled pointer speeds.
fn speed_stats(speeds: &VecDeque<(Instant, f32)>) -> (f32, f32) {
    let count = speeds.len();
    if count == 0 {
        return (0.0, 0.0);
    }

    let mean = speeds.iter().map(|(_, speed)| *speed).sum::<f32>() / count as f32;
    if count < 2 {
        return (mean, 0.0);
    }

    let variance = speeds
        .iter()
        .map(|(_, speed)| {
            let delta = speed - mean;
            delta * delta
        })
        .sum::<f32>()
        / count as f32;

    (mean, variance.sqrt())
}

/// Records one pointer sample and returns it when the emission throttle allows.
fn note_pointer(x: f64, y: f64, now: Instant) -> Option<CursorSample> {
    let mut state = pointer().lock().expect("pointer telemetry lock poisoned");
    prune_speeds(&mut state.speeds, now);

    if let Some((last_x, last_y, at)) = state.last_position {
        note_move_direction(&mut state, last_x, x, now);
        let elapsed = now.duration_since(at).as_secs_f32();
        // Skip duplicate events at identical timestamps to avoid infinite speed.
        if elapsed > f32::EPSILON {
            let distance = ((x - last_x).powi(2) + (y - last_y).powi(2)).sqrt() as f32;
            state.speeds.push_back((now, distance / elapsed));
            while state.speeds.len() > POINTER_SAMPLES {
                state.speeds.pop_front();
            }
        }
    }

    state.last_position = Some((x, y, now));
    state.last_move = Some(now);

    let due = state
        .last_emit
        .is_none_or(|last| now.duration_since(last) >= CURSOR_THROTTLE);
    if !due {
        return None;
    }
    state.last_emit = Some(now);
    Some(CursorSample { x, y })
}

/// Recomputes the score from freshly pruned buffers and caches it.
fn refresh_pond_stress() -> f32 {
    let now = Instant::now();
    let score = {
        let mut typing_state = typing().lock().expect("telemetry lock poisoned");
        prune(&mut typing_state.keypresses, now);
        prune(&mut typing_state.backspaces, now);

        let mut pointer_state = pointer().lock().expect("pointer telemetry lock poisoned");
        prune_speeds(&mut pointer_state.speeds, now);
        prune_hesitations(&mut pointer_state.hesitations, now);
        prune_reversals(&mut pointer_state.reversals, now);

        stress_model().score(&typing_state, &pointer_state)
    };
    latest_stress().store(score.to_bits(), Ordering::Relaxed);
    score
}

/// Latest normalized stress score, refreshed every [`STRESS_INTERVAL`].
pub fn pond_stress() -> f32 {
    f32::from_bits(latest_stress().load(Ordering::Relaxed))
}

fn record(app: &AppHandle, event: Event) {
    let now = Instant::now();
    match event.event_type {
        EventType::KeyPress(key) => {
            let mut state = typing().lock().expect("telemetry lock poisoned");
            note_keypress(&mut state, key, now);
        }
        EventType::MouseMove { x, y } => {
            if let Some(sample) = note_pointer(x, y, now) {
                let _ = app.emit(CURSOR_EVENT, sample);
            }
        }
        EventType::ButtonRelease(Button::Left) => {
            let _ = app.emit(RELEASE_EVENT, ());
        }
        EventType::ButtonPress(Button::Left) => {
            let mut state = pointer().lock().expect("pointer telemetry lock poisoned");
            note_click(&mut state, now);
        }
        EventType::Wheel { delta_y, .. } => {
            if delta_y != 0 {
                note_scroll(now);
            }
        }
        _ => {}
    }
}

/// Starts the global input listener and the stress ticker.
///
/// `rdev`'s macOS backend resolves key names through Text Services Manager inside
/// the event-tap callback, and TSM asserts it runs on the main queue (a violation
/// traps with `EXC_BREAKPOINT`). The tap must therefore be installed from the main
/// thread; on a background thread every keypress would crash the app.
/// Requires Accessibility permission on macOS.
pub fn start(app: AppHandle) {
    let _ = startup();
    let listener_app = app.clone();
    let run_listener = move || {
        if let Err(error) = listen(move |event| record(&listener_app, event)) {
            eprintln!("[telemetry] global input listener stopped: {error:?}");
        }
    };

    // macOS needs the tap on the main queue (see above), but `listen` blocks for
    // the lifetime of the app, so on Windows it must run on its own thread —
    // blocking the main thread there starves the event loop and the app crashes.
    #[cfg(target_os = "macos")]
    {
        if let Err(error) = app.run_on_main_thread(run_listener) {
            eprintln!("[telemetry] could not start listener on main thread: {error}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    std::thread::spawn(run_listener);

    std::thread::spawn(move || loop {
        std::thread::sleep(STRESS_INTERVAL);
        let score = refresh_pond_stress();
        let _ = app.emit(STRESS_EVENT, PondStress { score });
    });
}

pub fn snapshot() -> TypingSnapshot {
    let now = Instant::now();
    let mut state = typing().lock().expect("telemetry lock poisoned");

    // A stale run of backspaces is no longer a loop.
    if state
        .last_backspace
        .is_some_and(|stamp| now.duration_since(stamp) > BACKSPACE_STREAK_TIMEOUT)
    {
        state.backspace_streak = 0;
    }
    prune(&mut state.keypresses, now);
    prune(&mut state.backspaces, now);

    TypingSnapshot {
        keys_in_window: state.keypresses.len(),
        backspaces_in_window: state.backspaces.len(),
        backspace_streak: state.backspace_streak,
    }
}

/// Seconds since the most recent keyboard or mouse event, falling back to launch
/// time so a freshly started app still ages into idleness.
pub fn seconds_since_last_input() -> u64 {
    let now = Instant::now();
    let last_key = typing()
        .lock()
        .expect("telemetry lock poisoned")
        .last_keypress;
    let last_move = pointer()
        .lock()
        .expect("pointer telemetry lock poisoned")
        .last_move;
    let latest = match (last_key, last_move) {
        (Some(key), Some(moved)) => key.max(moved),
        (Some(key), None) => key,
        (None, Some(moved)) => moved,
        (None, None) => startup(),
    };
    now.duration_since(latest).as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_drops_entries_outside_window() {
        let now = Instant::now();
        let mut queue = VecDeque::new();
        queue.push_back(now - Duration::from_secs(31));
        queue.push_back(now - Duration::from_secs(5));
        prune(&mut queue, now);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn scroll_prune_uses_ninety_second_window() {
        let now = Instant::now();
        let mut state = ScrollTelemetry::default();
        state.events.push_back(now - Duration::from_secs(91));
        state.events.push_back(now - Duration::from_secs(89));
        prune_scrolls(&mut state, now);
        assert_eq!(state.events.len(), 1);
    }

    #[test]
    fn speed_stats_empty_is_zero() {
        assert_eq!(speed_stats(&VecDeque::new()), (0.0, 0.0));
    }

    #[test]
    fn speed_stats_mean_and_deviation() {
        let now = Instant::now();
        let mut speeds = VecDeque::new();
        speeds.push_back((now, 10.0));
        speeds.push_back((now, 20.0));
        let (mean, jitter) = speed_stats(&speeds);
        assert!((mean - 15.0).abs() < 1e-3);
        assert!((jitter - 5.0).abs() < 1e-3);
    }

    #[test]
    fn hesitation_stats_counts_threshold_and_means_all() {
        let now = Instant::now();
        let mut hesitations = VecDeque::new();
        hesitations.push_back((now, 0.5));
        hesitations.push_back((now, 2.5));
        hesitations.push_back((now, 4.0));
        let (hesitant, mean) = hesitation_stats(&hesitations);
        assert_eq!(hesitant, 2);
        assert!((mean - (0.5 + 2.5 + 4.0) / 3.0).abs() < 1e-3);
    }

    #[test]
    fn hesitation_stats_empty() {
        assert_eq!(hesitation_stats(&VecDeque::new()), (0, 0.0));
    }

    #[test]
    fn note_click_records_stillness_and_resets_anchor() {
        let mut state = PointerTelemetry::default();
        let base = Instant::now();
        state.last_position = Some((10.0, 20.0, base - Duration::from_secs(3)));
        note_click(&mut state, base);
        assert_eq!(state.hesitations.len(), 1);
        let dwell = state.hesitations.front().unwrap().1;
        assert!((dwell - 3.0).abs() < 1e-3);
        // Anchor resets to this click, so an immediate second click has ~0 dwell.
        note_click(&mut state, base + Duration::from_millis(100));
        assert!(state.hesitations.back().unwrap().1 < 0.2);
        assert_eq!(state.last_move, Some(base + Duration::from_millis(100)));
    }

    #[test]
    fn note_click_without_prior_move_is_ignored() {
        let mut state = PointerTelemetry::default();
        let now = Instant::now();
        note_click(&mut state, now);
        assert!(state.hesitations.is_empty());
        assert_eq!(state.last_move, Some(now));
    }

    #[test]
    fn direction_reversals_count_back_and_forth() {
        let mut state = PointerTelemetry::default();
        let base = Instant::now();
        // +20, -20, +20, -20 => a reversal on every step after the first.
        let steps = [0.0, 20.0, 0.0, 20.0, 0.0];
        let mut last = 0.0;
        for (i, x) in steps.iter().enumerate() {
            note_move_direction(&mut state, last, *x, base + Duration::from_millis(i as u64 * 100));
            last = *x;
        }
        assert_eq!(state.reversals.len(), 3);
    }

    #[test]
    fn tiny_moves_do_not_count_as_reversals() {
        let mut state = PointerTelemetry::default();
        let base = Instant::now();
        let steps = [0.0, 2.0, 0.0, 2.0, 0.0];
        let mut last = 0.0;
        for (i, x) in steps.iter().enumerate() {
            note_move_direction(&mut state, last, *x, base + Duration::from_millis(i as u64 * 100));
            last = *x;
        }
        assert!(state.reversals.is_empty());
    }

    #[test]
    fn stress_is_zero_when_idle() {
        let score = WeightedStress.score(&TypingTelemetry::default(), &PointerTelemetry::default());
        assert_eq!(score, 0.0);
    }

    #[test]
    fn stress_weights_key_pressure() {
        let now = Instant::now();
        let mut typing = TypingTelemetry::default();
        // Half of MAX_KEYS_PER_WINDOW (180) => key_pressure 0.5, weighted 0.35.
        for _ in 0..90 {
            typing.keypresses.push_back(now);
        }
        let score = WeightedStress.score(&typing, &PointerTelemetry::default());
        assert!((score - 0.25 * 0.5).abs() < 1e-3);
    }

    #[test]
    fn stress_clamps_to_one_when_saturated() {
        let now = Instant::now();
        let mut typing = TypingTelemetry::default();
        for _ in 0..400 {
            typing.keypresses.push_back(now);
        }
        for _ in 0..60 {
            typing.backspaces.push_back(now);
        }
        let mut pointer = PointerTelemetry::default();
        // Alternating speeds => high mean and high jitter, both saturate.
        for i in 0..120 {
            pointer
                .speeds
                .push_back((now, if i % 2 == 0 { 6000.0 } else { 0.0 }));
        }
        // Long dwells saturate the hesitation component too.
        for _ in 0..3 {
            pointer.hesitations.push_back((now, 4.0));
        }
        // A saturated reversal count saturates the oscillation component.
        for _ in 0..8 {
            pointer.reversals.push_back(now);
        }
        let score = WeightedStress.score(&typing, &pointer);
        assert!((score - 1.0).abs() < 1e-5);
    }

    #[test]
    fn backspace_streak_counts_within_timeout() {
        let mut state = TypingTelemetry::default();
        let base = Instant::now();
        for i in 0..5 {
            note_keypress(&mut state, Key::Backspace, base + Duration::from_millis(i * 100));
        }
        assert_eq!(state.backspace_streak, 5);
        assert_eq!(state.backspaces.len(), 5);
    }

    #[test]
    fn backspace_streak_resets_after_gap() {
        let mut state = TypingTelemetry::default();
        let base = Instant::now();
        note_keypress(&mut state, Key::Backspace, base);
        note_keypress(
            &mut state,
            Key::Backspace,
            base + BACKSPACE_STREAK_TIMEOUT + Duration::from_secs(1),
        );
        assert_eq!(state.backspace_streak, 1);
    }

    #[test]
    fn normal_key_resets_backspace_streak() {
        let mut state = TypingTelemetry::default();
        let base = Instant::now();
        note_keypress(&mut state, Key::Backspace, base);
        note_keypress(&mut state, Key::Backspace, base + Duration::from_millis(100));
        note_keypress(&mut state, Key::KeyA, base + Duration::from_millis(200));
        assert_eq!(state.backspace_streak, 0);
    }
}
