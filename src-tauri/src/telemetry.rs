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

use rdev::{listen, Event, EventType, Key};
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

// Per-window values that saturate their component of the stress score.
const MAX_KEYS_PER_WINDOW: f32 = 180.0; // 6 keys/s sustained
const MAX_BACKSPACES_PER_WINDOW: f32 = 24.0; // 0.8 backspaces/s sustained
const MAX_MEAN_SPEED: f32 = 3000.0; // px/s
const MAX_JITTER: f32 = 2000.0; // px/s std dev

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
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TypingSnapshot {
    pub keys_in_window: usize,
    pub backspaces_in_window: usize,
    /// `None` until the first keypress since launch.
    pub seconds_since_last_key: Option<u64>,
    /// Current run of consecutive backspaces (0 if the run ended).
    pub backspace_streak: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PointerSnapshot {
    /// `None` until the first mouse movement since launch.
    pub seconds_since_motion: Option<u64>,
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

        (key_pressure * 0.35
            + backspace_pressure * 0.25
            + velocity_pressure * 0.15
            + jitter_pressure * 0.25)
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

/// Latest score in `f32` bits, readable without taking a lock.
fn latest_stress() -> &'static AtomicU32 {
    static STRESS: AtomicU32 = AtomicU32::new(0);
    &STRESS
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
        seconds_since_last_key: state
            .last_keypress
            .map(|stamp| now.duration_since(stamp).as_secs()),
        backspace_streak: state.backspace_streak,
    }
}

pub fn pointer_snapshot() -> PointerSnapshot {
    let now = Instant::now();
    let state = pointer().lock().expect("pointer telemetry lock poisoned");

    PointerSnapshot {
        seconds_since_motion: state
            .last_move
            .map(|stamp| now.duration_since(stamp).as_secs()),
    }
}
