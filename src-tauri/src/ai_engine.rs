//! Stress scoring, LLM prompt pipeline, and intervention event emission.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::telemetry::{self, ScrollSnapshot, TypingSnapshot};
use crate::window_tracker::{self, WindowSnapshot};

const EVALUATION_INTERVAL: Duration = Duration::from_secs(10);

/// Evaluation cadence; `FROG_EVAL_SECS` overrides it for fast e2e tests.
fn evaluation_interval() -> Duration {
    std::env::var("FROG_EVAL_SECS")
        .ok()
        .and_then(|secs| secs.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(EVALUATION_INTERVAL)
}
/// Don't nudge the user more often than this.
const INTERVENTION_COOLDOWN: Duration = Duration::from_secs(45);

/// Doomscroll gate: minutes on one feed window, and wheel events in the last
/// 90 seconds. Both have env overrides for fast e2e checks.
const DOOMSCROLL_SECONDS: u64 = 4 * 60;
const DOOMSCROLL_SCROLLS: usize = 40;
/// Short-form video (YouTube Shorts) moves much faster: half a minute of
/// swiping is enough, with far fewer notches.
const SHORTS_SECONDS: u64 = 30;
const SHORTS_SCROLLS: usize = 6;
/// Any more typing than this means they are working, not scrolling.
const DOOMSCROLL_MAX_KEYS: usize = 10;

fn env_secs(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|secs| secs.parse().ok())
        .unwrap_or(fallback)
}

fn doomscroll_seconds() -> u64 {
    env_secs("FROG_DOOMSCROLL_SECS", DOOMSCROLL_SECONDS)
}

fn shorts_seconds() -> u64 {
    env_secs("FROG_SHORTS_SECS", SHORTS_SECONDS)
}

fn doomscroll_scrolls() -> usize {
    std::env::var("FROG_DOOMSCROLL_SCROLLS")
        .ok()
        .and_then(|count| count.parse().ok())
        .unwrap_or(DOOMSCROLL_SCROLLS)
}

/// Back-and-forth cursor reversals needed within the oscillation window before
/// it counts as stress. Set high — ordinary cursor moves flip direction often.
const OSCILLATION_MIN_REVERSALS: usize = 14;

fn shorts_scrolls() -> usize {
    std::env::var("FROG_SHORTS_SCROLLS")
        .ok()
        .and_then(|count| count.parse().ok())
        .unwrap_or(SHORTS_SCROLLS)
}

/// True when the feed is short-form video (YouTube Shorts or anything with
/// "shorts" in the title) and the user is flipping through it fast enough to
/// qualify for the 30-second rule. Flips count as well as wheel notches:
/// every new short retitles the tab, and touchpad/trackpoint scrolling often
/// never reaches the wheel hook.
fn is_short_form_scroll(windows: &WindowSnapshot, scrolls: &ScrollSnapshot) -> bool {
    let short_form_app = windows.active_app.as_ref().is_some_and(|app| {
        let title = app.title.to_lowercase();
        title.contains("youtube") || title.contains("shorts")
    });
    windows.on_feed
        && short_form_app
        && windows.feed_seconds >= shorts_seconds()
        && (scrolls.scrolls_in_30s >= shorts_scrolls()
            || windows.content_flips >= shorts_scrolls())
}

const EVENT_NAME: &str = "trigger-intervention";

const GEMINI_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const DEFAULT_MODEL: &str = "gemini-2.5-flash";
const LLM_TIMEOUT: Duration = Duration::from_secs(15);

const SYSTEM_PROMPT: &str = "You are The Feeling Frog, an empathetic, cozy amphibian desktop companion. \
The user is showing signs of burnout at their computer; you notice the pattern but never scold. \
Speak gently and warmly, with occasional subtle frog charm (a soft *ribbit* now and then) — never annoying, \
never nagging, never a lecture. Never use pet names or terms of endearment (no \"little friend\", \"dear\", \
\"buddy\", \"my friend\", \"little one\"); address the user plainly and directly. You are given a `detected` \
field describing exactly what gave them away; weave that detail into your message in a light, humorous way \
(for example, \"forty backspaces?\") so the user can tell what you noticed. Reply with a single JSON object \
matching the schema: \"message\" is one warm, grounding sentence (max 28 words) that playfully names the \
detected detail and helps the user feel settled, and \"suggestedAction\" is the soothing micro-action that \
fits it best.";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrogInterventionPayload {
    pub id: String,
    /// One of `frantic_typing`, `backspace_loop`, `tab_thrashing`, `idle_freeze`.
    pub trigger: String,
    /// One of `low`, `medium`, `high`.
    pub severity: String,
    pub message: String,
    /// One of `belly_breathe`, `visual_grounding`.
    pub suggested_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_app: Option<String>,
}

fn severity_for(score: u8) -> &'static str {
    if score >= 70 {
        "high"
    } else if score >= 40 {
        "medium"
    } else {
        "low"
    }
}

fn next_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    format!("frog-{millis}")
}

fn build_intervention(
    typing: &TypingSnapshot,
    windows: &WindowSnapshot,
    scrolls: &ScrollSnapshot,
) -> Option<FrogInterventionPayload> {
    // Normalized 0.0..=1.0 score computed by the telemetry engine and folded into
    // the window snapshot.
    let score = (windows.stress_score * 100.0).round() as u8;
    let active_app = windows.active_app.as_ref().map(|app| app.app_name.clone());

    // Short-form video earns the fast 30-second rule; everything else uses the
    // four-minute doomscroll gate.
    let short_form = is_short_form_scroll(windows, scrolls);

    let (trigger, message, suggested_action) = if short_form
        && typing.keys_in_window < DOOMSCROLL_MAX_KEYS
    {
        // Checked first: flipping Shorts looks like cursor thrashing to the
        // other detectors, and this is the more specific signal.
        let flips = windows.content_flips.max(scrolls.scrolls_in_30s);
        (
            "doomscroll",
            format!(
                "*ribbit* {flips} flips in half a minute of Shorts. That is not rest, that is a slot machine. Close the tab and let your eyes land on something real."
            ),
            "visual_grounding",
        )
    } else if typing.backspaces_in_window >= 36
        || typing.backspace_streak >= 8
    {
        (
            "backspace_loop",
            format!(
                "*ribbit* You've hit backspace {} times in the last 30 seconds. The sentence will still be there. Let's pause on the lily pad.",
                typing.backspaces_in_window
            ),
            "belly_breathe",
        )
    } else if typing.keys_in_window >= 180 {
        (
            "frantic_typing",
            format!(
                "*ribbit* {} keystrokes in 30 seconds — your keys are hopping faster than me. Let's slow the pond down for one breath.",
                typing.keys_in_window
            ),
            "belly_breathe",
        )
    } else if windows.hesitant_clicks >= 3 {
        (
            "cursor_hesitation",
            format!(
                "*ribbit* That cursor rested a while before clicking — {} hesitant clicks in the last 30 seconds. Your pond has plenty of lily pads; take one slow breath and pick one.",
                windows.hesitant_clicks
            ),
            "belly_breathe",
        )
    } else if windows.direction_reversals >= OSCILLATION_MIN_REVERSALS {
        (
            "cursor_oscillation",
            format!(
                "*ribbit* Your cursor is hopping back and forth across the pond — {} reversals in a few seconds. When you can't pick a lily pad, start with the smallest one and breathe.",
                windows.direction_reversals
            ),
            "belly_breathe",
        )
    } else if windows.on_feed
        && typing.keys_in_window < DOOMSCROLL_MAX_KEYS
        && windows.feed_seconds >= doomscroll_seconds()
        && scrolls.scrolls_in_window >= doomscroll_scrolls()
    {
        let minutes = (windows.feed_seconds / 60).max(1);
        (
            "doomscroll",
            format!(
                "*ribbit* {minutes} minutes of feed, {} scrolls, and not one word typed. That app is not a lily pad. Put it down and let your eyes rest on something that isn't glowing. I'm not asking twice.",
                scrolls.scrolls_in_window
            ),
            "visual_grounding",
        )
    } else if windows.tab_thrashing {
        (
            "tab_thrashing",
            format!(
                "*ribbit* {} window switches in 20 seconds. Your focus is doing laps. One lily pad at a time.",
                windows.switches_in_window
            ),
            "visual_grounding",
        )
    } else if windows.idle_freeze {
        (
            "idle_freeze",
            "*ribbit* Two and a half minutes, no keys and no mouse, and the editor still has you. Look away from the screen and let the pond settle.".to_string(),
            "visual_grounding",
        )
    } else {
        return None;
    };

    Some(FrogInterventionPayload {
        id: next_id(),
        trigger: trigger.to_string(),
        severity: severity_for(score).to_string(),
        message,
        suggested_action: suggested_action.to_string(),
        active_app,
    })
}

/// Human-readable version of what tripped the detector, fed to the model so it
/// can call out the exact signal (e.g. "40 backspaces in 30 seconds").
fn detection_detail(
    trigger: &str,
    typing: &TypingSnapshot,
    windows: &WindowSnapshot,
    scrolls: &ScrollSnapshot,
) -> String {
    match trigger {
        "backspace_loop" => format!(
            "{} backspaces in the last 30 seconds",
            typing.backspaces_in_window
        ),
        "frantic_typing" => format!(
            "{} keystrokes in the last 30 seconds",
            typing.keys_in_window
        ),
        "cursor_hesitation" => format!(
            "{} hesitant clicks (avg {:.1}s of stillness) in the last 30 seconds",
            windows.hesitant_clicks, windows.mean_hesitation_secs
        ),
        "cursor_oscillation" => format!(
            "{} back-and-forth cursor reversals in the last 10 seconds",
            windows.direction_reversals
        ),
        "tab_thrashing" => format!(
            "{} window switches in the last 20 seconds",
            windows.switches_in_window
        ),
        "doomscroll" => {
            let app = windows
                .active_app
                .as_ref()
                .map(|app| app.app_name.clone())
                .unwrap_or_else(|| "a feed".to_string());
            if is_short_form_scroll(windows, scrolls) {
                format!(
                    "{} flips in the last 30 seconds of short-form video on {app}",
                    windows.content_flips.max(scrolls.scrolls_in_30s)
                )
            } else {
                format!(
                    "{} minutes on {app} with {} scrolls in the last 90 seconds and no typing",
                    windows.feed_seconds / 60,
                    scrolls.scrolls_in_window
                )
            }
        }
        "idle_freeze" => "2.5 minutes with no keyboard or mouse input".to_string(),
        other => other.to_string(),
    }
}

/// What the model is asked to return.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LlmReply {
    message: String,
    suggested_action: String,
}

fn cooldown() -> &'static Mutex<Option<Instant>> {
    static COOLDOWN: Mutex<Option<Instant>> = Mutex::new(None);
    &COOLDOWN
}

// Test hook (disabled): interval override for repeat-trigger testing.
// fn intervention_cooldown() -> Duration {
//     std::env::var("FROG_COOLDOWN_SECS")
//         .ok()
//         .and_then(|secs| secs.parse().ok())
//         .map(Duration::from_secs)
//         .unwrap_or(INTERVENTION_COOLDOWN)
// }

fn suggested_action_for(trigger: &str) -> &'static str {
    match trigger {
        "tab_thrashing" | "idle_freeze" | "doomscroll" => "visual_grounding",
        _ => "belly_breathe",
    }
}

/// Reads `GEMINI_API_KEY`, loading `.env.local` when the app was started without it.
fn api_key() -> Option<String> {
    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        return Some(key);
    }
    for file in [".env.local", "../.env.local"] {
        if dotenvy::from_filename(file).is_ok() {
            if let Ok(key) = std::env::var("GEMINI_API_KEY") {
                return Some(key);
            }
        }
    }
    None
}

/// Builds the Gemini request body: Feeling Frog persona, the observed trigger,
/// and the active window title. The response is constrained to the payload JSON.
fn build_llm_request(trigger: &str, window_title: &str, detected: &str) -> serde_json::Value {
    let observation = serde_json::json!({
        "trigger": trigger,
        "activeWindowTitle": window_title,
        "detected": detected,
    });

    // Doomscrolling is the one time the frog is allowed to be stern.
    let system = if trigger == "doomscroll" {
        format!(
            "{SYSTEM_PROMPT} For this trigger the user has been doomscrolling for far too long. \
Drop the gentle tone: be playfully stern and disappointed, like a friend who has had enough. \
Tell them plainly to close the feed, in one short sentence, still with a little frog flourish."
        )
    } else {
        SYSTEM_PROMPT.to_string()
    };

    serde_json::json!({
        "system_instruction": { "parts": [{ "text": system }] },
        "contents": [{
            "role": "user",
            "parts": [{ "text": observation.to_string() }]
        }],
        "generationConfig": {
            "temperature": 0.7,
            "responseMimeType": "application/json",
            "responseSchema": {
                "type": "OBJECT",
                "properties": {
                    "message": { "type": "STRING" },
                    "suggestedAction": {
                        "type": "STRING",
                        "enum": ["belly_breathe", "visual_grounding"]
                    }
                },
                "required": ["message", "suggestedAction"]
            }
        }
    })
}

async fn ask_llm(trigger: &str, window_title: &str, detected: &str) -> Result<LlmReply, String> {
    let key = api_key().ok_or("GEMINI_API_KEY not configured")?;
    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let url = format!("{GEMINI_ENDPOINT}/{model}:generateContent?key={key}");

    // Test hook (disabled): LLM latency timing.
    // let started = Instant::now();
    let body: serde_json::Value = reqwest::Client::new()
        .post(url)
        .json(&build_llm_request(trigger, window_title, detected))
        .timeout(LLM_TIMEOUT)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;

    let text = body["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or("model returned no text")?;

    let reply = serde_json::from_str::<LlmReply>(text)
        .map_err(|error| format!("invalid model JSON: {error}"))?;
    // eprintln!("[ai_engine] LLM round-trip: {:.0?}", started.elapsed());
    Ok(reply)
}

/// Fires a Feeling Frog intervention once telemetry crosses a burnout threshold.
///
/// Throttled to one nudge per five minutes. Sends only the trigger type and the
/// active window title to the LLM; the deterministic copy is the fallback when
/// the model or its API key is unavailable. Emits `trigger-intervention`.
pub async fn trigger_frog_intervention(
    app: AppHandle,
    trigger: String,
    window_title: String,
    detected: String,
    fallback_message: String,
) -> Option<FrogInterventionPayload> {
    {
        let mut last = cooldown()
            .lock()
            .expect("intervention cooldown lock poisoned");
        if last.is_some_and(|stamp| stamp.elapsed() < INTERVENTION_COOLDOWN) {
            return None;
        }
        *last = Some(Instant::now());
    }

    let score = (telemetry::pond_stress() * 100.0).round() as u8;
    let mut payload = FrogInterventionPayload {
        id: next_id(),
        trigger: trigger.clone(),
        severity: severity_for(score).to_string(),
        message: fallback_message,
        suggested_action: suggested_action_for(&trigger).to_string(),
        active_app: Some(window_title.clone()),
    };

    match ask_llm(&trigger, &window_title, &detected).await {
        Ok(reply) => {
            payload.message = reply.message;
            // The trigger→activity mapping wins for doomscroll so the eyes-off-screen
            // guide always opens; other triggers let the model pick its soothing action.
            if trigger != "doomscroll" {
                payload.suggested_action = reply.suggested_action;
            }
        }
        Err(error) => {
            eprintln!("[ai_engine] intervention LLM unavailable, using fallback: {error}");
        }
    }

    if let Err(error) = app.emit(EVENT_NAME, &payload) {
        eprintln!("[ai_engine] could not emit intervention: {error}");
        return None;
    }
    eprintln!("[ai_engine] emitted trigger={}", payload.trigger);

    Some(payload)
}

/// Starts the periodic evaluation loop that emits `trigger-intervention` events.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let forced = std::env::var("FROG_FORCE_TRIGGER").ok();
        if let Some(trigger) = forced.clone() {
            std::thread::sleep(Duration::from_secs(2));
            fire_forced(&app, &trigger);
        }

        loop {
            std::thread::sleep(evaluation_interval());

            let typing = telemetry::snapshot();
            let windows = window_tracker::snapshot();
            let scrolls = telemetry::scroll_snapshot();
            eprintln!(
                "[ai_engine] tick keys={} backspaces={} streak={} switches={} thrash={} idle={} hesit={} rev={} feed={}({}s) flips={} scrolls30={} scrolls90={} stress={:.2}",
                typing.keys_in_window,
                typing.backspaces_in_window,
                typing.backspace_streak,
                windows.switches_in_window,
                windows.tab_thrashing,
                windows.idle_freeze,
                windows.hesitant_clicks,
                windows.direction_reversals,
                windows.on_feed,
                windows.feed_seconds,
                windows.content_flips,
                scrolls.scrolls_in_30s,
                scrolls.scrolls_in_window,
                windows.stress_score
            );

            let Some(candidate) = build_intervention(&typing, &windows, &scrolls) else {
                continue;
            };

            let window_title = windows
                .active_app
                .as_ref()
                .map(|app| app.title.clone())
                .unwrap_or_default();
            let detected = detection_detail(&candidate.trigger, &typing, &windows, &scrolls);

            tauri::async_runtime::block_on(trigger_frog_intervention(
                app.clone(),
                candidate.trigger,
                window_title,
                detected,
                candidate.message,
            ));
        }
    });
}

fn fire_forced(app: &AppHandle, trigger: &str) -> bool {
    let fallback = format!("*ribbit* Dev trigger: {trigger}. Take one slow breath with me.");
    tauri::async_runtime::block_on(trigger_frog_intervention(
        app.clone(),
        trigger.to_string(),
        "Final Essay - Google Docs".to_string(),
        "40 backspaces in the last 30 seconds".to_string(),
        fallback,
    ))
    .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_bands() {
        assert_eq!(severity_for(0), "low");
        assert_eq!(severity_for(39), "low");
        assert_eq!(severity_for(40), "medium");
        assert_eq!(severity_for(69), "medium");
        assert_eq!(severity_for(70), "high");
        assert_eq!(severity_for(100), "high");
    }

    #[test]
    fn no_intervention_when_calm() {
        let windows = WindowSnapshot {
            stress_score: 0.5,
            ..Default::default()
        };
        assert!(build_intervention(&TypingSnapshot::default(), &windows, &ScrollSnapshot::default()).is_none());
    }

    #[test]
    fn backspace_loop_detected_by_count() {
        let typing = TypingSnapshot {
            backspaces_in_window: 36,
            ..Default::default()
        };
        let payload = build_intervention(&typing, &WindowSnapshot::default(), &ScrollSnapshot::default()).unwrap();
        assert_eq!(payload.trigger, "backspace_loop");
        assert_eq!(payload.suggested_action, "belly_breathe");
    }

    #[test]
    fn backspace_loop_detected_by_streak() {
        let typing = TypingSnapshot {
            backspace_streak: 8,
            ..Default::default()
        };
        let payload = build_intervention(&typing, &WindowSnapshot::default(), &ScrollSnapshot::default()).unwrap();
        assert_eq!(payload.trigger, "backspace_loop");
    }

    #[test]
    fn frantic_typing_at_threshold() {
        let typing = TypingSnapshot {
            keys_in_window: 180,
            ..Default::default()
        };
        let payload = build_intervention(&typing, &WindowSnapshot::default(), &ScrollSnapshot::default()).unwrap();
        assert_eq!(payload.trigger, "frantic_typing");
        assert_eq!(payload.suggested_action, "belly_breathe");
    }

    #[test]
    fn cursor_hesitation_detected() {
        let windows = WindowSnapshot {
            hesitant_clicks: 3,
            mean_hesitation_secs: 2.4,
            ..Default::default()
        };
        let payload = build_intervention(&TypingSnapshot::default(), &windows, &ScrollSnapshot::default()).unwrap();
        assert_eq!(payload.trigger, "cursor_hesitation");
        assert_eq!(payload.suggested_action, "belly_breathe");
    }

    #[test]
    fn detection_detail_mentions_hesitation() {
        let windows = WindowSnapshot {
            hesitant_clicks: 4,
            mean_hesitation_secs: 2.5,
            ..Default::default()
        };
        let detail = detection_detail("cursor_hesitation", &TypingSnapshot::default(), &windows, &ScrollSnapshot::default());
        assert!(detail.contains("4 hesitant clicks"));
    }

    #[test]
    fn cursor_oscillation_detected() {
        let windows = WindowSnapshot {
            direction_reversals: OSCILLATION_MIN_REVERSALS,
            ..Default::default()
        };
        let payload = build_intervention(&TypingSnapshot::default(), &windows, &ScrollSnapshot::default()).unwrap();
        assert_eq!(payload.trigger, "cursor_oscillation");
        assert_eq!(payload.suggested_action, "belly_breathe");
    }

    #[test]
    fn detection_detail_mentions_oscillation() {
        let windows = WindowSnapshot {
            direction_reversals: OSCILLATION_MIN_REVERSALS + 1,
            ..Default::default()
        };
        let detail = detection_detail("cursor_oscillation", &TypingSnapshot::default(), &windows, &ScrollSnapshot::default());
        assert!(detail.contains("15 back-and-forth"));
    }

    #[test]
    fn tab_thrashing_detected() {
        let windows = WindowSnapshot {
            tab_thrashing: true,
            switches_in_window: 9,
            ..Default::default()
        };
        let payload = build_intervention(&TypingSnapshot::default(), &windows, &ScrollSnapshot::default()).unwrap();
        assert_eq!(payload.trigger, "tab_thrashing");
        assert_eq!(payload.suggested_action, "visual_grounding");
    }

    #[test]
    fn idle_freeze_detected() {
        let windows = WindowSnapshot {
            idle_freeze: true,
            ..Default::default()
        };
        let payload = build_intervention(&TypingSnapshot::default(), &windows, &ScrollSnapshot::default()).unwrap();
        assert_eq!(payload.trigger, "idle_freeze");
    }

    #[test]
    fn backspace_has_priority_over_other_signals() {
        let typing = TypingSnapshot {
            backspaces_in_window: 36,
            keys_in_window: 200,
            ..Default::default()
        };
        let windows = WindowSnapshot {
            tab_thrashing: true,
            idle_freeze: true,
            ..Default::default()
        };
        let payload = build_intervention(&typing, &windows, &ScrollSnapshot::default()).unwrap();
        assert_eq!(payload.trigger, "backspace_loop");
    }

    #[test]
    fn detection_detail_reports_counts() {
        let typing = TypingSnapshot {
            backspaces_in_window: 40,
            ..Default::default()
        };
        let detail = detection_detail("backspace_loop", &typing, &WindowSnapshot::default(), &ScrollSnapshot::default());
        assert_eq!(detail, "40 backspaces in the last 30 seconds");
    }

    #[test]
    fn suggested_actions_map_by_trigger() {
        assert_eq!(suggested_action_for("backspace_loop"), "belly_breathe");
        assert_eq!(suggested_action_for("frantic_typing"), "belly_breathe");
        assert_eq!(suggested_action_for("cursor_hesitation"), "belly_breathe");
        assert_eq!(suggested_action_for("cursor_oscillation"), "belly_breathe");
        assert_eq!(suggested_action_for("tab_thrashing"), "visual_grounding");
        assert_eq!(suggested_action_for("idle_freeze"), "visual_grounding");
        assert_eq!(suggested_action_for("doomscroll"), "visual_grounding");
        assert_eq!(suggested_action_for("unknown"), "belly_breathe");
    }

    #[test]
    fn doomscroll_detected_on_long_feed_scroll_without_typing() {
        let windows = WindowSnapshot {
            on_feed: true,
            feed_seconds: DOOMSCROLL_SECONDS,
            ..Default::default()
        };
        let scrolls = ScrollSnapshot {
            scrolls_in_window: DOOMSCROLL_SCROLLS,
            scrolls_in_30s: 0,
        };
        let payload = build_intervention(
            &TypingSnapshot::default(),
            &windows,
            &scrolls,
        )
        .unwrap();
        assert_eq!(payload.trigger, "doomscroll");
        assert_eq!(payload.suggested_action, "visual_grounding");
        assert!(payload.message.contains("scrolls"));
    }

    #[test]
    fn shorts_trigger_after_thirty_seconds_of_swiping() {
        let windows = WindowSnapshot {
            on_feed: true,
            feed_seconds: SHORTS_SECONDS,
            active_app: Some(crate::window_tracker::ActiveApp {
                app_name: "Google Chrome".to_string(),
                title: "Funny cat - YouTube".to_string(),
            }),
            ..Default::default()
        };
        let scrolls = ScrollSnapshot {
            scrolls_in_window: SHORTS_SCROLLS,
            scrolls_in_30s: SHORTS_SCROLLS,
        };
        let payload =
            build_intervention(&TypingSnapshot::default(), &windows, &scrolls).unwrap();
        assert_eq!(payload.trigger, "doomscroll");
        assert!(payload.message.contains("Shorts"));
    }

    #[test]
    fn shorts_trigger_from_title_flips_without_any_wheel_events() {
        let windows = WindowSnapshot {
            on_feed: true,
            feed_seconds: SHORTS_SECONDS,
            content_flips: SHORTS_SCROLLS,
            active_app: Some(crate::window_tracker::ActiveApp {
                app_name: "Microsoft Edge".to_string(),
                title: "Some short - YouTube".to_string(),
            }),
            ..Default::default()
        };
        let no_wheel = ScrollSnapshot::default();
        let payload =
            build_intervention(&TypingSnapshot::default(), &windows, &no_wheel).unwrap();
        assert_eq!(payload.trigger, "doomscroll");
        assert!(payload.message.contains("Shorts"));
    }

    #[test]
    fn shorts_rule_needs_fast_swipes_and_a_short_form_window() {
        let fast = ScrollSnapshot {
            scrolls_in_window: SHORTS_SCROLLS,
            scrolls_in_30s: SHORTS_SCROLLS,
        };
        let slow = ScrollSnapshot {
            scrolls_in_window: SHORTS_SCROLLS - 1,
            scrolls_in_30s: SHORTS_SCROLLS - 1,
        };
        let youtube = WindowSnapshot {
            on_feed: true,
            feed_seconds: SHORTS_SECONDS,
            active_app: Some(crate::window_tracker::ActiveApp {
                app_name: "Google Chrome".to_string(),
                title: "Funny cat - YouTube".to_string(),
            }),
            ..Default::default()
        };
        // A few swipes over half a minute is browsing, not Shorts doomscrolling.
        assert!(build_intervention(&TypingSnapshot::default(), &youtube, &slow).is_none());

        // The same cadence on TikTok keeps the four-minute rule.
        let tiktok = WindowSnapshot {
            active_app: Some(crate::window_tracker::ActiveApp {
                app_name: "TikTok".to_string(),
                title: "For You".to_string(),
            }),
            ..youtube.clone()
        };
        assert!(build_intervention(&TypingSnapshot::default(), &tiktok, &fast).is_none());
    }

    #[test]
    fn doomscroll_ignored_when_typing_short_or_not_a_feed() {
        let scrolls = ScrollSnapshot {
            scrolls_in_window: DOOMSCROLL_SCROLLS,
            ..Default::default()
        };
        let feed = WindowSnapshot {
            on_feed: true,
            feed_seconds: DOOMSCROLL_SECONDS,
            ..Default::default()
        };

        let typing = TypingSnapshot {
            keys_in_window: DOOMSCROLL_MAX_KEYS,
            ..Default::default()
        };
        assert!(build_intervention(&typing, &feed, &scrolls).is_none());

        let short = WindowSnapshot {
            feed_seconds: DOOMSCROLL_SECONDS - 1,
            ..feed.clone()
        };
        assert!(build_intervention(&TypingSnapshot::default(), &short, &scrolls).is_none());

        let elsewhere = WindowSnapshot {
            on_feed: false,
            ..feed.clone()
        };
        assert!(build_intervention(&TypingSnapshot::default(), &elsewhere, &scrolls).is_none());

        let barely_scrolled = ScrollSnapshot {
            scrolls_in_window: DOOMSCROLL_SCROLLS - 1,
            ..Default::default()
        };
        assert!(
            build_intervention(&TypingSnapshot::default(), &feed, &barely_scrolled).is_none()
        );
    }

    #[test]
    fn doomscroll_prompt_turns_sterner() {
        let calm = build_llm_request("backspace_loop", "Docs", "40 backspaces");
        let stern = build_llm_request("doomscroll", "Reddit", "9 minutes of scrolling");
        let calm_system = calm["system_instruction"]["parts"][0]["text"]
            .as_str()
            .unwrap();
        let stern_system = stern["system_instruction"]["parts"][0]["text"]
            .as_str()
            .unwrap();
        assert!(stern_system.contains("doomscrolling"));
        assert!(stern_system.len() > calm_system.len());
    }

    #[test]
    fn llm_request_carries_persona_observation_and_schema() {
        let request = build_llm_request("backspace_loop", "Final Essay", "40 backspaces");
        let system = request["system_instruction"]["parts"][0]["text"]
            .as_str()
            .unwrap();
        assert!(system.contains("Feeling Frog"));
        let observation = request["contents"][0]["parts"][0]["text"]
            .as_str()
            .unwrap();
        assert!(observation.contains("Final Essay"));
        assert!(observation.contains("backspace_loop"));
        assert_eq!(
            request["generationConfig"]["responseMimeType"],
            "application/json"
        );
        let actions = request["generationConfig"]["responseSchema"]["properties"]
            ["suggestedAction"]["enum"]
            .as_array()
            .unwrap();
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn ids_are_prefixed_and_increase() {
        let first = next_id();
        std::thread::sleep(Duration::from_millis(3));
        let second = next_id();
        assert!(first.starts_with("frog-"));
        assert_ne!(first, second);
    }
}
