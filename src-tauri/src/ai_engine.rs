//! Stress scoring, LLM prompt pipeline, and intervention event emission.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::telemetry::{self, TypingSnapshot};
use crate::window_tracker::{self, WindowSnapshot};

const EVALUATION_INTERVAL: Duration = Duration::from_secs(20);
/// Don't nudge the user more often than this.
const INTERVENTION_COOLDOWN: Duration = Duration::from_secs(5 * 60);

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
    /// One of `belly_breathe`, `lily_pad_step`, `pond_stretch`.
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
) -> Option<FrogInterventionPayload> {
    // Normalized 0.0..=1.0 score computed by the telemetry engine and folded into
    // the window snapshot.
    let score = (windows.stress_score * 100.0).round() as u8;
    let active_app = windows.active_app.as_ref().map(|app| app.app_name.clone());

    let (trigger, message, suggested_action) = if typing.backspaces_in_window >= 24
        || typing.backspace_streak >= 6
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
            "pond_stretch",
        )
    } else if windows.tab_thrashing {
        (
            "tab_thrashing",
            format!(
                "*ribbit* {} window switches in 20 seconds. Your focus is doing laps. One lily pad at a time.",
                windows.switches_in_window
            ),
            "lily_pad_step",
        )
    } else if windows.idle_freeze {
        (
            "idle_freeze",
            "*ribbit* Six minutes, no keys and no mouse, and the editor still has you. Stretch your legs and let the pond settle.".to_string(),
            "pond_stretch",
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
        "tab_thrashing" => format!(
            "{} window switches in the last 20 seconds",
            windows.switches_in_window
        ),
        "idle_freeze" => "6 minutes with no keyboard or mouse input".to_string(),
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
        "backspace_loop" => "belly_breathe",
        "tab_thrashing" => "lily_pad_step",
        _ => "pond_stretch",
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

    serde_json::json!({
        "system_instruction": { "parts": [{ "text": SYSTEM_PROMPT }] },
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
                        "enum": ["belly_breathe", "lily_pad_step", "pond_stretch"]
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
            payload.suggested_action = reply.suggested_action;
        }
        Err(error) => {
            eprintln!("[ai_engine] intervention LLM unavailable, using fallback: {error}");
        }
    }

    if let Err(error) = app.emit(EVENT_NAME, &payload) {
        eprintln!("[ai_engine] could not emit intervention: {error}");
        return None;
    }

    Some(payload)
}

/// Starts the periodic evaluation loop that emits `trigger-intervention` events.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // Test hook (disabled): FROG_FORCE_TRIGGER=backspace_loop fired every tick.
        // let forced = std::env::var("FROG_FORCE_TRIGGER").ok();
        // if let Some(trigger) = forced.clone() {
        //     std::thread::sleep(Duration::from_secs(12));
        //     fire_forced(&app, &trigger);
        // }

        loop {
            std::thread::sleep(EVALUATION_INTERVAL);

            let typing = telemetry::snapshot();
            let windows = window_tracker::snapshot();
            // eprintln!(
            //     "[ai_engine] tick keys={} backspaces={} streak={} switches={} thrash={} idle={} stress={:.2}",
            //     typing.keys_in_window,
            //     typing.backspaces_in_window,
            //     typing.backspace_streak,
            //     windows.switches_in_window,
            //     windows.tab_thrashing,
            //     windows.idle_freeze,
            //     windows.stress_score
            // );

            let Some(candidate) = build_intervention(&typing, &windows) else {
                continue;
            };

            let window_title = windows
                .active_app
                .as_ref()
                .map(|app| app.title.clone())
                .unwrap_or_default();
            let detected = detection_detail(&candidate.trigger, &typing, &windows);

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

// Test hook (disabled): forced intervention helper.
// fn fire_forced(app: &AppHandle, trigger: &str) -> bool {
//     let fallback = format!("*ribbit* Dev trigger: {trigger}. Take one slow breath with me.");
//     tauri::async_runtime::block_on(trigger_frog_intervention(
//         app.clone(),
//         trigger.to_string(),
//         "Final Essay - Google Docs".to_string(),
//         fallback,
//     ))
//     .is_some()
// }
