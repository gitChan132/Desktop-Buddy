//! Stress scoring, LLM prompt pipeline, and intervention event emission.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::telemetry::{self, TypingSnapshot};
use crate::window_tracker::{self, WindowSnapshot};

const EVALUATION_INTERVAL: Duration = Duration::from_secs(20);
/// Don't nudge the user more often than this.
const MIN_INTERVENTION_GAP: Duration = Duration::from_secs(75);

const EVENT_NAME: &str = "frog-intervention";

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
            "*ribbit* Your keys are hopping faster than me. Let's slow the pond down for one breath.".to_string(),
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

/// LLM prompt pipeline.
///
/// Produces the JSON instructions for the model that rewrites `message`. When an
/// endpoint/API key is configured, send this prompt and replace the message with
/// the model's response; until then the deterministic copy is emitted directly.
pub fn build_prompt(payload: &FrogInterventionPayload) -> String {
    serde_json::json!({
        "system": "You are Feeling Frog, a warm desktop companion. Write one short, gentle intervention (max 28 words) that helps the user recover from burnout. Be playful but never judgmental. End with a concrete micro-action.",
        "response_format": {
            "type": "json_object",
            "schema": {
                "message": "string",
                "suggestedAction": "belly_breathe | lily_pad_step | pond_stretch"
            }
        },
        "observation": {
            "trigger": payload.trigger,
            "severity": payload.severity,
            "activeApp": payload.active_app,
            "draftMessage": payload.message
        }
    })
    .to_string()
}

/// Starts the periodic evaluation loop that emits `frog-intervention` events.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_intervention: Option<Instant> = None;

        loop {
            std::thread::sleep(EVALUATION_INTERVAL);

            let typing = telemetry::snapshot();
            let windows = window_tracker::snapshot();
            let Some(payload) = build_intervention(&typing, &windows) else {
                continue;
            };

            let cooled_down = last_intervention
                .is_none_or(|stamp| stamp.elapsed() >= MIN_INTERVENTION_GAP);
            if !cooled_down {
                continue;
            }

            // Prompt is ready for the LLM; deterministic copy is the fallback.
            let _prompt = build_prompt(&payload);
            if app.emit(EVENT_NAME, payload).is_ok() {
                last_intervention = Some(Instant::now());
            }
        }
    });
}
