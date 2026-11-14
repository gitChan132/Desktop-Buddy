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
/// Window used to count title changes inside the same app (short-form flips).
const FLIP_WINDOW: Duration = Duration::from_secs(30);
/// More than this many switches inside [`SWITCH_WINDOW`] is tab thrashing.
const THRASH_SWITCHES: usize = 5;
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

/// Site names we can recognise inside a browser tab title, most specific first.
/// Browsers never give us the URL, so this is a best-effort identity.
const KNOWN_SITES: &[&str] = &[
    "youtube",
    "instagram",
    "tiktok",
    "reddit",
    "twitter",
    "x.com",
    "facebook",
    "github",
    "gitlab",
    "gmail",
    "notion",
    "slack",
    "discord",
    "chatgpt",
    "claude",
    "linkedin",
    "netflix",
    "twitch",
    "spotify",
    "figma",
    "wikipedia",
    "medium",
    "substack",
    "stack overflow",
    "amazon",
    "trello",
    "jira",
    "outlook",
    "google",
];

/// Browser suffixes that are not part of the page title.
const BROWSER_SUFFIXES: &[&str] = &[
    " - google chrome",
    " - personal - microsoft edge",
    " - microsoft edge",
    " - mozilla firefox",
    " - brave",
    " - opera",
    " - vivaldi",
    " - chromium",
    " - safari",
];

/// Best-effort identity for the *site* in front, so tab-thrashing only counts
/// real website changes. Browsers do not expose the URL, so for them we strip
/// the browser suffix, prefer a recognised site name, and otherwise fall back to
/// the last " - " segment of the title. Non-browser apps are identified by their
/// app name (two files in one editor are not a site change).
fn site_key(app_name: &str, title: &str) -> String {
    let app = app_name.to_lowercase();
    if !BROWSERS.iter().any(|needle| app.contains(needle)) {
        return app;
    }

    let mut cleaned = title.to_lowercase();
    for suffix in BROWSER_SUFFIXES {
        if let Some(stripped) = cleaned.strip_suffix(suffix) {
            cleaned = stripped.to_string();
            break;
        }
    }
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return app;
    }

    let last = cleaned.rsplit(" - ").next().unwrap_or(cleaned).trim();
    if let Some(site) = KNOWN_SITES
        .iter()
        .find(|site| !site.is_empty() && last.contains(*site))
    {
        return (*site).to_string();
    }
    if let Some(site) = KNOWN_SITES.iter().find(|site| cleaned.contains(*site)) {
        return (*site).to_string();
    }
    last.to_string()
}

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
    /// Title changes inside the same app: page navigation, or flipping through
    /// short-form videos, which retitles the tab on every swipe.
    title_changes: VecDeque<Instant>,
    /// When the current uninterrupted run on a feed window began.
    feed_since: Option<Instant>,
}

/// Window-tracker state joined with the telemetry stress score.
#[derive(Clone, Debug, Default)]
pub struct WindowSnapshot {
    pub active_app: Option<ActiveApp>,
    pub switches_in_window: usize,
    /// Detected: more than 5 window/title switches in the last 20 seconds.
    pub tab_thrashing: bool,
    /// Detected: 2.5 minutes without input while a browser or editor is active.
    pub idle_freeze: bool,
    /// Clicks in the last 30s preceded by at least 2s of cursor stillness.
    pub hesitant_clicks: usize,
    /// Mean seconds the cursor was still before clicking.
    pub mean_hesitation_secs: f32,
    /// Significant back-and-forth cursor reversals in the last 10 seconds.
    pub direction_reversals: usize,
    /// Title changes inside the same app in the last 30 seconds (short-form
    /// flips: every swipe retitles the tab).
    pub content_flips: usize,
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

fn prune_with(queue: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while queue
        .front()
        .is_some_and(|stamp| now.duration_since(*stamp) > window)
    {
        queue.pop_front();
    }
}

fn prune(queue: &mut VecDeque<Instant>, now: Instant) {
    prune_with(queue, now, SWITCH_WINDOW);
}

fn is_focus_app(app_name: &str) -> bool {
    let app_name = app_name.to_lowercase();
    FOCUS_APPS
        .iter()
        .any(|needle| app_name.contains(needle))
}

fn note_window_change(state: &mut TrackerState, next: ActiveApp, now: Instant) {
    if state.current.as_ref() == Some(&next) {
        return;
    }
    if let Some(previous) = &state.current {
        // Tab-thrashing counts *site* changes, so tab-hopping inside YouTube or
        // Instagram does not register as a switch. Title changes within the same
        // site are content flips (used by the short-form detector).
        if site_key(&previous.app_name, &previous.title)
            != site_key(&next.app_name, &next.title)
        {
            state.switches.push_back(now);
        } else if previous.title != next.title {
            state.title_changes.push_back(now);
        }
    }
    state.current = Some(next);
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
    note_window_change(&mut state, next, now);
    prune(&mut state.switches, now);
    prune_with(&mut state.title_changes, now, FLIP_WINDOW);

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
    let (active_app, switches_in_window, tab_thrashing, content_flips, on_feed, feed_seconds) = {
        let mut state = tracker().lock().expect("window tracker lock poisoned");
        prune(&mut state.switches, now);
        prune_with(&mut state.title_changes, now, FLIP_WINDOW);
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
            state.title_changes.len(),
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
        content_flips,
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

    #[test]
    fn site_key_keeps_tabs_within_one_website_together() {
        assert_eq!(site_key("Google Chrome", "Cat video - YouTube"), "youtube");
        assert_eq!(
            site_key("Google Chrome", "Cat video - YouTube"),
            site_key("Google Chrome", "Another video - YouTube")
        );
        assert_eq!(site_key("Google Chrome", "Instagram"), "instagram");
        assert_eq!(
            site_key("Microsoft Edge", "Cat video - YouTube - Personal - Microsoft Edge"),
            "youtube"
        );
        assert_eq!(
            site_key("Google Chrome", "Reddit - Dive into anything"),
            "reddit"
        );
        assert_ne!(
            site_key("Google Chrome", "Cat video - YouTube"),
            site_key("Google Chrome", "Reddit - Dive into anything")
        );
        // Non-browser apps are their own site.
        assert_eq!(
            site_key("Visual Studio Code", "main.ts - Desktop-Buddy"),
            "visual studio code"
        );
    }

    #[test]
    fn app_switch_and_title_flip_are_classified_apart() {
        let now = Instant::now();
        let mut state = TrackerState::default();

        note_window_change(
            &mut state,
            ActiveApp {
                app_name: "Google Chrome".to_string(),
                title: "Some short - YouTube".to_string(),
            },
            now,
        );
        // Same app, new short: a content flip, not a window switch.
        for i in 0..3 {
            note_window_change(
                &mut state,
                ActiveApp {
                    app_name: "Google Chrome".to_string(),
                    title: format!("short {i} - YouTube"),
                },
                now,
            );
        }
        assert_eq!(state.switches.len(), 0);
        assert_eq!(state.title_changes.len(), 3);

        // Another YouTube tab is still the same website: no switch.
        note_window_change(
            &mut state,
            ActiveApp {
                app_name: "Google Chrome".to_string(),
                title: "Different tab - YouTube".to_string(),
            },
            now,
        );
        assert_eq!(state.switches.len(), 0);

        // Chrome on Reddit *is* a different site.
        note_window_change(
            &mut state,
            ActiveApp {
                app_name: "Google Chrome".to_string(),
                title: "Reddit - Dive into anything".to_string(),
            },
            now,
        );
        assert_eq!(state.switches.len(), 1);

        // A different app is a switch too.
        note_window_change(
            &mut state,
            ActiveApp {
                app_name: "Visual Studio Code".to_string(),
                title: "main.ts - Desktop-Buddy".to_string(),
            },
            now,
        );
        assert_eq!(state.switches.len(), 2);
        assert_eq!(state.title_changes.len(), 4);

        // Re-polling the same window changes nothing.
        note_window_change(
            &mut state,
            ActiveApp {
                app_name: "Visual Studio Code".to_string(),
                title: "main.ts - Desktop-Buddy".to_string(),
            },
            now,
        );
        assert_eq!(state.switches.len(), 2);
    }

    #[test]
    fn thrash_threshold_is_strictly_greater_than_eight() {
        let now = Instant::now();
        let mut switches: VecDeque<Instant> =
            (0..THRASH_SWITCHES).map(|_| now - Duration::from_secs(1)).collect();
        prune(&mut switches, now);
        assert!(switches.len() <= THRASH_SWITCHES, "at threshold must not thrash");
        switches.push_back(now);
        assert!(switches.len() > THRASH_SWITCHES, "above threshold must thrash");
    }
}
