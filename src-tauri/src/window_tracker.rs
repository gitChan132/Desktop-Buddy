//! Active-window polling and app/tab thrashing detection.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use active_win_pos_rs::get_active_window;

const POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// Window used to count context switches.
const SWITCH_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveApp {
    pub app_name: String,
    pub title: String,
}

#[derive(Default)]
struct TrackerState {
    current: Option<ActiveApp>,
    switches: VecDeque<Instant>,
}

#[derive(Clone, Debug, Default)]
pub struct WindowSnapshot {
    pub active_app: Option<ActiveApp>,
    pub switches_in_window: usize,
}

fn tracker() -> &'static Mutex<TrackerState> {
    static TRACKER: OnceLock<Mutex<TrackerState>> = OnceLock::new();
    TRACKER.get_or_init(|| Mutex::new(TrackerState::default()))
}

fn prune(queue: &mut VecDeque<Instant>, now: Instant) {
    while queue
        .front()
        .is_some_and(|stamp| now.duration_since(*stamp) > SWITCH_WINDOW)
    {
        queue.pop_front();
    }
}

fn poll_once() {
    let Ok(window) = get_active_window() else {
        return;
    };
    let next = ActiveApp {
        app_name: window.app_name,
        title: window.title,
    };

    let now = Instant::now();
    let mut state = tracker().lock().expect("window tracker lock poisoned");
    prune(&mut state.switches, now);

    // Only count transitions between distinct app/window pairs, not repeated polls.
    if state.current.as_ref() != Some(&next) {
        if state.current.is_some() {
            state.switches.push_back(now);
        }
        state.current = Some(next);
    }
}

/// Starts polling the foreground window on a background thread.
pub fn start() {
    std::thread::spawn(|| loop {
        poll_once();
        std::thread::sleep(POLL_INTERVAL);
    });
}

pub fn snapshot() -> WindowSnapshot {
    let now = Instant::now();
    let mut state = tracker().lock().expect("window tracker lock poisoned");
    prune(&mut state.switches, now);

    WindowSnapshot {
        active_app: state.current.clone(),
        switches_in_window: state.switches.len(),
    }
}
