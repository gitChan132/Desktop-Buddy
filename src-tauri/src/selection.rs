//! Watches the system-wide text selection through macOS Accessibility (AX).
//!
//! Every tick we ask the focused app's focused element for its selected text.
//! When a long selection stays put for two ticks we tell the frontend so the
//! frog can offer a summary. No clipboard access, no synthetic keystrokes.

use std::collections::hash_map::DefaultHasher;
use std::ffi::c_void;
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::{CFBoolean, CFRetained, CFString};
use tauri::{AppHandle, Emitter};

use crate::chat::SelectionReadyPayload;

const POLL_INTERVAL: Duration = Duration::from_millis(900);
/// Selections shorter than this are not a "wall of text".
const MIN_CHARS: usize = 400;
/// Cap what we hand to the model (and the frontend).
const MAX_CHARS: usize = 8000;
/// Re-offering the same selection is only interesting much later.
const SAME_TEXT_COOLDOWN: Duration = Duration::from_secs(120);
/// Any two offers are at least this far apart.
const GLOBAL_COOLDOWN: Duration = Duration::from_secs(10);

const EVENT_SELECTION_READY: &str = "selection-ready";
const EVENT_SELECTION_UNAVAILABLE: &str = "selection-unavailable";

type AXUIElementRef = *mut c_void;
type CFTypeRef = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: *const c_void,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: *const c_void,
        value: CFTypeRef,
    ) -> i32;
    fn AXIsProcessTrusted() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
    fn CFBooleanGetValue(value: CFTypeRef) -> bool;
}

/// Copies one attribute off an AX element; the returned value is owned (+1).
#[cfg(target_os = "macos")]
unsafe fn copy_attribute(element: AXUIElementRef, name: &str) -> Option<CFTypeRef> {
    let attribute = CFString::from_str(name);
    let mut value: CFTypeRef = std::ptr::null();
    let error = AXUIElementCopyAttributeValue(
        element,
        CFRetained::as_ptr(&attribute).as_ptr() as *const c_void,
        &mut value,
    );
    if error == 0 && !value.is_null() {
        Some(value)
    } else {
        None
    }
}

unsafe fn release(value: CFTypeRef) {
    if !value.is_null() {
        unsafe { CFRelease(value) };
    }
}

/// Reads a CFString into an owned Rust string.
#[cfg(target_os = "macos")]
unsafe fn string_from(value: CFTypeRef) -> Option<String> {
    let string = NonNull::new(value as *mut CFString)?;
    // Display copies through CFStringGetBytes, so it also handles the
    // UTF-16-backed strings Chromium apps hand back; `as_str_unchecked` only
    // works for 8-bit-backed ones and silently returns None.
    Some(unsafe { string.as_ref() }.to_string())
}

/// Chromium apps (VS Code, Slack, browsers) publish no accessibility tree until
/// an assistive client sets this flag — before that, even the focused element
/// is missing. Unsupported on native apps, which is harmless.
#[cfg(target_os = "macos")]
unsafe fn enable_web_accessibility(app: AXUIElementRef) {
    for name in ["AXManualAccessibility", "AXEnhancedUserInterface"] {
        let attribute = CFString::from_str(name);
        unsafe {
            // Chrome ignores one and honours the other, Electron the reverse;
            // whichever is unsupported just answers with an error.
            AXUIElementSetAttributeValue(
                app,
                CFRetained::as_ptr(&attribute).as_ptr() as *const c_void,
                CFBoolean::new(true) as *const CFBoolean as CFTypeRef,
            );
        }
    }
}

/// The currently selected text in whatever app has focus.
///
/// We ask the frontmost app directly instead of going through the system-wide
/// element: `AXFocusedApplication` on the system-wide element answers
/// `kAXErrorCannotComplete` (-25204) for processes that are not bundled GUI
/// apps — which includes the dev binary — so it silently returned nothing.
#[cfg(target_os = "macos")]
fn selected_text(last_manual_pid: &mut i32) -> Option<String> {
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        // The menu-bar owner, not the frontmost app: our always-on-top panel
        // can become frontmost while the user is still working elsewhere.
        let front = workspace
            .menuBarOwningApplication()
            .or_else(|| workspace.frontmostApplication())?;
        let pid = front.processIdentifier();
        if pid <= 0 || pid as u32 == std::process::id() {
            return None;
        }

        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return None;
        }
        // Set this once per app: re-sending it on every poll makes Chromium
        // rebuild its tree and hand back an empty selection each time.
        if *last_manual_pid != pid {
            enable_web_accessibility(app);
            *last_manual_pid = pid;
        }
        let focused = copy_attribute(app, "AXFocusedUIElement");
        release(app as CFTypeRef);
        let focused = focused?;

        // Never read selections out of password-style fields.
        if let Some(secure) = copy_attribute(focused as AXUIElementRef, "AXSecureTextField") {
            let is_secure = CFBooleanGetValue(secure);
            release(secure);
            if is_secure {
                release(focused);
                return None;
            }
        }

        let text = copy_attribute(focused as AXUIElementRef, "AXSelectedText");
        release(focused);
        string_from(text?)
    }
}

#[cfg(not(target_os = "macos"))]
fn selected_text(_last_manual_pid: &mut i32) -> Option<String> {
    None
}

fn digest(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Polls the selection on a background thread for the life of the app.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        if unsafe { !AXIsProcessTrusted() } {
            // Surface once; the frontend shows a hint that links to the
            // Accessibility settings pane.
            let _ = app.emit(EVENT_SELECTION_UNAVAILABLE, ());
        }

        // Last selection seen on the previous tick, and how many ticks it has
        // survived so far — a drag in progress keeps changing it.
        let mut previous: Option<u64> = None;
        let mut stable_ticks = 0u8;
        let mut last_offered: Option<u64> = None;
        let mut last_offer_at: Option<Instant> = None;
        let mut manual_pid: i32 = -1;

        loop {
            std::thread::sleep(POLL_INTERVAL);
            let selected = selected_text(&mut manual_pid);
            let trimmed = selected
                .as_deref()
                .map(str::trim)
                .filter(|text| text.chars().count() >= MIN_CHARS);
            let hash = trimmed.map(digest);

            if hash != previous || hash.is_none() {
                previous = hash;
                stable_ticks = 0;
                continue;
            }
            stable_ticks = stable_ticks.saturating_add(1);
            if stable_ticks < 2 {
                continue;
            }

            let Some(text) = trimmed else { continue };
            let hash = hash.expect("stable selection has a digest");
            let now = Instant::now();
            let recently_offered = last_offer_at.is_some_and(|at| {
                let same = last_offered == Some(hash);
                let cooldown = if same { SAME_TEXT_COOLDOWN } else { GLOBAL_COOLDOWN };
                now.duration_since(at) < cooldown
            });
            if recently_offered {
                continue;
            }

            let clipped: String = text.chars().take(MAX_CHARS).collect();
            let payload = SelectionReadyPayload {
                words: clipped.split_whitespace().count(),
                text: clipped,
            };
            if app.emit(EVENT_SELECTION_READY, payload).is_ok() {
                last_offered = Some(hash);
                last_offer_at = Some(now);
            }
        }
    });
}
