//! Active-window polling, tab-thrashing and idle-freeze detection.
//!
//! Every 2 seconds the foreground app/window is sampled into a rolling 20-second
//! switch history. A snapshot combines that history with the telemetry engine's
//! stress score and input recency, so `ai_engine` sees one stress picture.
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use active_win_pos_rs::get_active_window;

use crate::telemetry;

const POLL_INTERVAL: Duration = Duration::from_millis(2000);
/// Window used to count window/title switches.
const SWITCH_WINDOW: Duration = Duration::from_secs(20);
/// More than this many switches inside [`SWITCH_WINDOW`] is tab thrashing.
const THRASH_SWITCHES: usize = 8;
/// Zero keyboard/mouse input this long while a focus app is active is an idle freeze.
const IDLE_FREEZE: Duration = Duration::from_secs(150);

/// Idle-freeze threshold; `FROG_IDLE_FREEZE_SECS` overrides it for fast e2e tests.
fn idle_freeze() -> Duration {
    std::env::var("FROG_IDLE_FREEZE_SECS")
        .ok()
        .and_then(|secs| secs.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(IDLE_FREEZE)
}

/// App names (lowercase substrings) where sustained zero input means stuck, not resting.
const FOCUS_APPS: &[&str] = &[
    // Browsers
    "chrome",
    "safari",
    "firefox",
    "arc",
    "edge",
    "brave",
    "opera",
    "vivaldi",
    "orion",
    "chromium",
    // Editors and IDEs
    "code",
    "cursor",
    "sublime",
    "zed",
    "xcode",
    "intellij",
    "webstorm",
    "pycharm",
    "android studio",
    "neovim",
    "vim",
    "emacs",
    "textmate",
    "nova",
];

/// Browser app names (lowercase substrings) whose window titles we inspect.
const BROWSERS: &[&str] = &[
    "chrome",
    "safari",
    "firefox",
    "arc",
    "edge",
    "brave",
    "opera",
    "vivaldi",
    "orion",
    "chromium",
];

/// Feed apps that are their own window, so the app name alone is enough.
/// Scope is deliberately tight: TikTok and Instagram only.
const FEED_APPS: &[&str] = &["tiktok", "instagram"];

/// Title hints that mean a browser tab is a feed. YouTube counts here because
/// it has no desktop app; "shorts"/"reels" catch the short-form feeds.
const FEED_TITLE_HINTS: &[&str] = &["instagram", "youtube", "tiktok", "shorts", "reels"];

/// True when the foreground window looks like a social feed: either a dedicated
/// feed app, or a browser showing a feed tab. Only the title is matched, never
/// page contents.
fn is_feed_window(app_name: &str, title: &str) -> bool {
    let app_name = app_name.to_lowercase();
    let title = title.to_lowercase();
    let feed_app = FEED_APPS.iter().any(|needle| app_name.contains(needle));
    let browser = BROWSERS.iter().any(|needle| app_name.contains(needle));
    feed_app || (browser && FEED_TITLE_HINTS.iter().any(|needle| title.contains(needle)))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveApp {
    pub app_name: String,
    pub title: String,
}

#[derive(Default)]
struct TrackerState {
    current: Option<ActiveApp>,
    switches: VecDeque<Instant>,
    /// When the current uninterrupted run on a feed window began.
    feed_since: Option<Instant>,
}

/// Window-tracker state joined with the telemetry stress score.
#[derive(Clone, Debug, Default)]
pub struct WindowSnapshot {
    pub active_app: Option<ActiveApp>,
    pub switches_in_window: usize,
    /// Detected: more than 8 window/title switches in the last 20 seconds.
    pub tab_thrashing: bool,
    /// Detected: 2.5 minutes without input while a browser or editor is active.
    pub idle_freeze: bool,
    /// Clicks in the last 30s preceded by at least 2s of cursor stillness.
    pub hesitant_clicks: usize,
    /// Mean seconds the cursor was still before clicking.
    pub mean_hesitation_secs: f32,
    /// Significant back-and-forth cursor reversals in the last 10 seconds.
    pub direction_reversals: usize,
    /// Detected: the foreground window looks like a social feed.
    pub on_feed: bool,
    /// Seconds the same feed window has been in front (0 when not on a feed).
    pub feed_seconds: u64,
    /// Normalized (`0.0..=1.0`) pond ripple stress score from the telemetry engine.
    pub stress_score: f32,
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

fn is_focus_app(app_name: &str) -> bool {
    let app_name = app_name.to_lowercase();
    FOCUS_APPS
        .iter()
        .any(|needle| app_name.contains(needle))
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

    // The feed clock survives title changes on the same feed, and resets the
    // moment focus lands anywhere else.
    let on_feed = state
        .current
        .as_ref()
        .is_some_and(|app| is_feed_window(&app.app_name, &app.title));
    if on_feed {
        state.feed_since.get_or_insert(now);
    } else {
        state.feed_since = None;
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
    let stress_score = telemetry::pond_stress();
    let seconds_since_input = telemetry::seconds_since_last_input();
    let hesitation = telemetry::hesitation_snapshot();
    let direction_reversals = telemetry::direction_reversals();

    let now = Instant::now();
    let (active_app, switches_in_window, tab_thrashing, on_feed, feed_seconds) = {
        let mut state = tracker().lock().expect("window tracker lock poisoned");
        prune(&mut state.switches, now);
        let on_feed = state
            .current
            .as_ref()
            .is_some_and(|app| is_feed_window(&app.app_name, &app.title));
        let feed_seconds = if on_feed {
            state
                .feed_since
                .map(|since| now.duration_since(since).as_secs())
                .unwrap_or(0)
        } else {
            0
        };
        (
            state.current.clone(),
            state.switches.len(),
            state.switches.len() > THRASH_SWITCHES,
            on_feed,
            feed_seconds,
        )
    };

    let idle_freeze = active_app
        .as_ref()
        .is_some_and(|app| is_focus_app(&app.app_name))
        && seconds_since_input >= idle_freeze().as_secs();

    WindowSnapshot {
        active_app,
        switches_in_window,
        tab_thrashing,
        idle_freeze,
        hesitant_clicks: hesitation.hesitant_clicks,
        mean_hesitation_secs: hesitation.mean_hesitation_secs,
        direction_reversals,
        on_feed,
        feed_seconds,
        stress_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_app_matches_browsers_and_editors() {
        assert!(is_focus_app("Google Chrome"));
        assert!(is_focus_app("Safari"));
        assert!(is_focus_app("Visual Studio Code"));
        assert!(is_focus_app("Cursor"));
        assert!(is_focus_app("Zed"));
    }

    #[test]
    fn focus_app_rejects_terminals_and_misc() {
        assert!(!is_focus_app("Terminal"));
        assert!(!is_focus_app("iTerm2"));
        assert!(!is_focus_app("Finder"));
        assert!(!is_focus_app(""));
    }

    #[test]
    fn prune_keeps_only_recent_switches() {
        let now = Instant::now();
        let mut switches = VecDeque::new();
        switches.push_back(now - Duration::from_secs(25));
        switches.push_back(now - Duration::from_secs(10));
        prune(&mut switches, now);
        assert_eq!(switches.len(), 1);
    }

    #[test]
    fn feed_window_matches_apps_and_browser_tabs() {
        assert!(is_feed_window("TikTok", "For You"));
        assert!(is_feed_window("Google Chrome", "Instagram"));
        assert!(is_feed_window("Microsoft Edge", "YouTube"));
        assert!(is_feed_window("Microsoft Edge", "Shorts"));
    }

    #[test]
    fn feed_window_ignores_editors_and_generic_pages() {
        assert!(!is_feed_window("Visual Studio Code", "reddit.ts - Desktop-Buddy"));
        assert!(!is_feed_window("Google Chrome", "Docs - Google Chrome"));
        assert!(!is_feed_window("Terminal", "instagram"));
        // Out of scope: only Instagram, YouTube and TikTok count.
        assert!(!is_feed_window("Google Chrome", "Reddit - Dive into anything"));
        assert!(!is_feed_window("Google Chrome", "Home / X"));
        assert!(!is_feed_window("", ""));
    }
}
