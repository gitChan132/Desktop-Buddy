//! Privacy-first input telemetry.
//!
//! Only timing metadata is recorded (counts of keypresses and backspaces inside a
//! sliding window). No key contents, text, or clipboard data ever leave this module.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rdev::{listen, Event, EventType, Key};

/// Sliding window used to compute typing rates.
const WINDOW: Duration = Duration::from_secs(10);

#[derive(Default)]
struct TypingTelemetry {
    keypresses: VecDeque<Instant>,
    backspaces: VecDeque<Instant>,
    last_keypress: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TypingSnapshot {
    pub keys_in_window: usize,
    pub backspaces_in_window: usize,
    /// `None` until the first keypress since launch.
    pub seconds_since_last_key: Option<u64>,
}

fn telemetry() -> &'static Mutex<TypingTelemetry> {
    static TELEMETRY: OnceLock<Mutex<TypingTelemetry>> = OnceLock::new();
    TELEMETRY.get_or_init(|| Mutex::new(TypingTelemetry::default()))
}

fn prune(queue: &mut VecDeque<Instant>, now: Instant) {
    while queue
        .front()
        .is_some_and(|stamp| now.duration_since(*stamp) > WINDOW)
    {
        queue.pop_front();
    }
}

fn record(event: Event) {
    let EventType::KeyPress(key) = event.event_type else {
        return;
    };

    let now = Instant::now();
    let mut state = telemetry().lock().expect("telemetry lock poisoned");
    prune(&mut state.keypresses, now);
    prune(&mut state.backspaces, now);
    state.keypresses.push_back(now);
    if matches!(key, Key::Backspace) {
        state.backspaces.push_back(now);
    }
    state.last_keypress = Some(now);
}

/// Starts the global key listener on a background thread.
/// Requires Accessibility permission on macOS.
pub fn start() {
    std::thread::spawn(|| {
        if let Err(error) = listen(record) {
            eprintln!("[telemetry] global key listener stopped: {error:?}");
        }
    });
}

pub fn snapshot() -> TypingSnapshot {
    let now = Instant::now();
    let mut state = telemetry().lock().expect("telemetry lock poisoned");
    prune(&mut state.keypresses, now);
    prune(&mut state.backspaces, now);

    TypingSnapshot {
        keys_in_window: state.keypresses.len(),
        backspaces_in_window: state.backspaces.len(),
        seconds_since_last_key: state
            .last_keypress
            .map(|stamp| now.duration_since(stamp).as_secs()),
    }
}
