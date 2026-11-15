mod ai_engine;
mod chat;
mod selection;
mod settings;
mod telemetry;
mod window_tracker;

use std::sync::Mutex;
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow,
};

/// Gap between the frog and the screen edges, in physical pixels.
const SCREEN_MARGIN: i32 = 12;
/// Gap between the tray icon and the control panel, in physical pixels.
const TRAY_GAP: i32 = 8;

const TRAY_ID: &str = "frog-tray";
const PANEL_LABEL: &str = "panel";
/// The big lilypad settings window, opened from the small popover.
const POND_LABEL: &str = "pond";
/// The small always-on-top trigger remote shown beside the frog.
const REMOTE_LABEL: &str = "remote";
const SETTINGS_EVENT: &str = "frog-settings";
/// Fired when the pond panel is shown so the frog can hop in again.
const PANEL_OPENED_EVENT: &str = "pond-panel-shown";

/// Holds the tray menu so the check marks can be synced from commands.
struct TrayMenu(tauri::menu::Menu<tauri::Wry>);

/// Distance from the window's bottom edge to the frog's feet, in CSS px. The
/// sprite button sits `bottom-3` (12px) above the window edge and the art ends
/// at 31/32 of a 32px cell rendered at 192px, i.e. 18px. Used to start him
/// resting on the taskbar, the same spot a throw drops him at, instead of
/// floating a sprite-box height above it.
const FROG_FEET_INSET_CSS: f64 = 18.0;

/// macOS: let the window float over other apps' full-screen Spaces.
/// `CanJoinAllSpaces` (set by `set_visible_on_all_workspaces`) keeps it on every
/// regular Space. `FullScreenAuxiliary` alone only lifts it over the owning
/// app's full-screen window, so raise the window level above the full-screen
/// window level as well — otherwise another app's full-screen Space hides it.
#[cfg(target_os = "macos")]
fn float_over_fullscreen_spaces(window: &WebviewWindow) -> tauri::Result<()> {
    use objc2_app_kit::{
        NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior,
    };

    let ptr = window.ns_window()? as *mut NSWindow;
    if ptr.is_null() {
        return Ok(());
    }
    // Tauri setup runs on the main thread; the window outlives this borrow.
    let ns_window: &NSWindow = unsafe { &*ptr };
    let behavior = ns_window.collectionBehavior()
        | NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary;
    ns_window.setCollectionBehavior(behavior);
    ns_window.setLevel(NSStatusWindowLevel);
    Ok(())
}

/// Anchors the frameless frog window to the bottom-right of the monitor work area,
/// with his feet resting on the bottom edge rather than the window frame.
fn position_bottom_right(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let size = window.outer_size()?;
    let scale = window.scale_factor()?;
    let feet_inset = (FROG_FEET_INSET_CSS * scale).round() as i32;

    let x = area.position.x + area.size.width as i32 - size.width as i32 - SCREEN_MARGIN;
    // The transparent window hangs a little under the taskbar so the visible
    // frog lands exactly where a thrown frog settles.
    let y = area.position.y + area.size.height as i32 - size.height as i32 + feet_inset;
    window.set_position(PhysicalPosition::new(x, y))
}

/// Builds the menu-bar / system-tray icon. Left-clicking the icon toggles the
/// control panel; right-clicking opens the menu with the same controls.
fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "panel", "Control panel", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &CheckMenuItem::with_id(
                app,
                "pause",
                "Pause nudges",
                true,
                settings::nudges_paused(),
                None::<&str>,
            )?,
            &CheckMenuItem::with_id(
                app,
                "sound",
                "Croak sounds",
                true,
                settings::sound_enabled(),
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "testing", "Test triggers…", true, None::<&str>)?,
            &MenuItem::with_id(app, "quit", "Quit Feeling Frog", true, None::<&str>)?,
        ],
    )?;

    app.manage(TrayMenu(menu.clone()));

    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Feeling Frog")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "panel" => toggle_panel(app, None),
            "pause" => {
                settings::set_nudges_paused(!settings::nudges_paused());
                sync_tray_checks(app);
                broadcast_settings(app);
            }
            "sound" => {
                settings::set_sound_enabled(!settings::sound_enabled());
                sync_tray_checks(app);
                broadcast_settings(app);
            }
            "testing" => open_remote(app.clone()),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                position,
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_panel(tray.app_handle(), Some(position));
            }
        })
        .build(app)?;
    Ok(())
}

/// Keeps the frog window in step with the settings window: the overlay is
/// always-on-top, so it has to step aside while the panel is up.
fn set_main_visible(app: &AppHandle, visible: bool) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = if visible { main.show() } else { main.hide() };
    }
}

/// Opens or hides the small control popover, anchoring it near the clicked
/// tray icon. The big pond window is opened from inside the popover.
fn toggle_panel(app: &AppHandle, cursor: Option<PhysicalPosition<f64>>) {
    let Some(panel) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        let _ = panel.hide();
        return;
    }
    match cursor {
        Some(cursor) => position_panel_near(&panel, cursor),
        None => position_panel_default(&panel),
    }
    let _ = panel.show();
    let _ = panel.set_focus();
}

/// Places the panel under a top tray icon (macOS menu bar) or above a bottom
/// one (Windows taskbar), clamped to the monitor work area.
fn position_panel_near(panel: &WebviewWindow, cursor: PhysicalPosition<f64>) {
    let Ok(size) = panel.outer_size() else {
        return;
    };
    let monitors = panel.available_monitors().unwrap_or_default();
    let monitor = monitors.iter().find(|monitor| {
        let position = monitor.position();
        let area = monitor.size();
        let x = cursor.x as i32;
        let y = cursor.y as i32;
        x >= position.x
            && y >= position.y
            && x < position.x + area.width as i32
            && y < position.y + area.height as i32
    });
    let Some(monitor) = monitor else {
        return;
    };

    let area = monitor.work_area();
    let middle = area.position.y + area.size.height as i32 / 2;
    let mut x = cursor.x as i32 - size.width as i32 / 2;
    let mut y = if cursor.y as i32 >= middle {
        cursor.y as i32 - size.height as i32 - TRAY_GAP
    } else {
        cursor.y as i32 + TRAY_GAP
    };

    let min_x = area.position.x + SCREEN_MARGIN;
    let max_x = area.position.x + area.size.width as i32 - size.width as i32 - SCREEN_MARGIN;
    let min_y = area.position.y + SCREEN_MARGIN;
    let max_y = area.position.y + area.size.height as i32 - size.height as i32 - SCREEN_MARGIN;
    if max_x > min_x {
        x = x.clamp(min_x, max_x);
    }
    if max_y > min_y {
        y = y.clamp(min_y, max_y);
    }
    let _ = panel.set_position(PhysicalPosition::new(x, y));
}

/// Fallback for opening the panel from the tray menu: top-right of the primary
/// monitor, just below the menu bar.
fn position_panel_default(panel: &WebviewWindow) {
    let Ok(Some(monitor)) = panel.primary_monitor() else {
        return;
    };
    let Ok(size) = panel.outer_size() else {
        return;
    };
    let area = monitor.work_area();
    let x = area.position.x + area.size.width as i32 - size.width as i32 - SCREEN_MARGIN;
    let y = area.position.y + SCREEN_MARGIN;
    let _ = panel.set_position(PhysicalPosition::new(x, y));
}

/// Centres the big pond window on the primary monitor.
fn position_pond_center(pond: &WebviewWindow) {
    let Ok(Some(monitor)) = pond.primary_monitor() else {
        return;
    };
    let Ok(size) = pond.outer_size() else {
        return;
    };
    let area = monitor.work_area();
    let x = area.position.x + (area.size.width as i32 - size.width as i32) / 2;
    let y = area.position.y + (area.size.height as i32 - size.height as i32) / 2;
    let _ = pond.set_position(PhysicalPosition::new(x, y));
}

/// Mirrors the current settings into the tray menu's check marks.
fn sync_tray_checks(app: &AppHandle) {
    let menu = &app.state::<TrayMenu>().0;
    let settings = settings::settings();
    for (id, checked) in [
        ("pause", settings.nudges_paused),
        ("sound", settings.sound_enabled),
    ] {
        if let Some(MenuItemKind::Check(item)) = menu.get(id) {
            let _ = item.set_checked(checked);
        }
    }
}

/// Pushes the current settings to every window so the panel and frog stay live.
fn broadcast_settings(app: &AppHandle) {
    let _ = app.emit(SETTINGS_EVENT, settings::settings());
}

#[tauri::command]
fn get_settings() -> settings::Settings {
    settings::settings()
}

#[tauri::command]
fn set_nudges_paused(app: AppHandle, paused: bool) {
    settings::set_nudges_paused(paused);
    sync_tray_checks(&app);
    broadcast_settings(&app);
}

#[tauri::command]
fn set_sound_enabled(app: AppHandle, enabled: bool) {
    settings::set_sound_enabled(enabled);
    sync_tray_checks(&app);
    broadcast_settings(&app);
}

#[tauri::command]
fn set_sound_volume(app: AppHandle, volume: f32) {
    settings::set_volume(volume);
    broadcast_settings(&app);
}

#[tauri::command]
fn set_mode(app: AppHandle, mode: String) {
    settings::set_mode(&mode);
    broadcast_settings(&app);
}

#[tauri::command]
fn set_frog_color(app: AppHandle, color: String) {
    settings::set_frog_color(&color);
    broadcast_settings(&app);
}

#[tauri::command]
fn set_selection_summaries(app: AppHandle, enabled: bool) {
    settings::set_selection_summaries(enabled);
    broadcast_settings(&app);
}

#[tauri::command]
fn set_trigger_enabled(app: AppHandle, trigger: String, enabled: bool) {
    settings::set_trigger_enabled(&trigger, enabled);
    broadcast_settings(&app);
}

#[tauri::command]
fn set_activity_enabled(app: AppHandle, activity: String, enabled: bool) {
    settings::set_activity_enabled(&activity, enabled);
    broadcast_settings(&app);
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Snapshot for the pond panel's status column.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FrogStats {
    stress: f32,
    focus: f32,
    keys: usize,
    backspaces: usize,
    switches: usize,
    hesitant_clicks: usize,
    reversals: usize,
    scrolls_30s: usize,
    on_feed: bool,
    feed_seconds: u64,
    seconds_since_input: u64,
    active_app: Option<String>,
    nudges_paused: bool,
    sound_enabled: bool,
}

#[tauri::command]
fn frog_stats() -> FrogStats {
    let typing = telemetry::snapshot();
    let windows = window_tracker::snapshot();
    let scrolls = telemetry::scroll_snapshot();

    // Focus proxy: many site/app switches or sitting in a feed means attention is
    // scattered; calm, single-window work keeps it near 1.0.
    let switch_pressure = (windows.switches_in_window as f32 / 5.0).clamp(0.0, 1.0);
    let distraction = switch_pressure * 0.6 + if windows.on_feed { 0.4 } else { 0.0 };

    FrogStats {
        stress: telemetry::pond_stress(),
        focus: (1.0 - distraction).clamp(0.0, 1.0),
        keys: typing.keys_in_window,
        backspaces: typing.backspaces_in_window,
        switches: windows.switches_in_window,
        hesitant_clicks: windows.hesitant_clicks,
        reversals: windows.direction_reversals,
        scrolls_30s: scrolls.scrolls_in_30s,
        on_feed: windows.on_feed,
        feed_seconds: windows.feed_seconds,
        seconds_since_input: telemetry::seconds_since_last_input(),
        active_app: windows.active_app.as_ref().map(|app| app.app_name.clone()),
        nudges_paused: settings::nudges_paused(),
        sound_enabled: settings::sound_enabled(),
    }
}

/// Where the frog currently sits on screen (physical px), so the panel can start
/// his hop from the right place.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FrogAnchor {
    x: f64,
    y: f64,
}

#[tauri::command]
fn frog_anchor(app: AppHandle) -> Option<FrogAnchor> {
    let main = app.get_webview_window("main")?;
    let position = main.outer_position().ok()?;
    let size = main.outer_size().ok()?;
    Some(FrogAnchor {
        // Centre of the visible frog, roughly 78% down the window.
        x: position.x as f64 + size.width as f64 / 2.0,
        y: position.y as f64 + size.height as f64 * 0.78,
    })
}

/// Which pond tab the popover last asked for, plus a sequence number so the
/// pond can notice repeat requests even when the tab is unchanged.
static POND_TAB: Mutex<String> = Mutex::new(String::new());
static POND_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static POND_SHOW_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn set_pond_tab(tab: &str) {
    if let Ok(mut current) = POND_TAB.lock() {
        current.clear();
        current.push_str(tab);
    }
    POND_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PondTab {
    tab: String,
    seq: u64,
    show_seq: u64,
}

#[tauri::command]
fn pond_tab() -> PondTab {
    PondTab {
        tab: POND_TAB
            .lock()
            .map(|tab| tab.clone())
            .unwrap_or_else(|_| "status".to_string()),
        seq: POND_SEQ.load(std::sync::atomic::Ordering::Relaxed),
        show_seq: POND_SHOW_SEQ.load(std::sync::atomic::Ordering::Relaxed),
    }
}

/// Opens the big pond window from the small popover: hide the popover and the
/// frog overlay, centre the pond, raise it and let the frog hop in.
#[tauri::command]
fn open_pond(app: AppHandle, tab: Option<String>) {
    if let Some(panel) = app.get_webview_window(PANEL_LABEL) {
        let _ = panel.hide();
    }
    let Some(pond) = app.get_webview_window(POND_LABEL) else {
        return;
    };
    position_pond_center(&pond);
    let _ = pond.show();
    let _ = pond.set_focus();
    let tab = tab.unwrap_or_else(|| "status".to_string());
    set_pond_tab(&tab);
    POND_SHOW_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let _ = app.emit(PANEL_OPENED_EVENT, tab);
}

/// Coordinates for the real frog's flight onto the pond lilypad.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JumpRequest {
    target_x: f64,
    target_y: f64,
    direction: String,
}

/// Latest jump request, kept so the frog overlay can fetch it if the event was
/// missed while its webview was idle (or before its listener was attached).
static POND_JUMP: Mutex<Option<JumpRequest>> = Mutex::new(None);
static POND_JUMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingJump {
    seq: u64,
    request: JumpRequest,
}

/// True while the real frog window is parked on the lilypad: he has landed,
/// hidden himself behind the pond, and will be dropped back down when the pond
/// closes.
static POND_FROG_ON_PAD: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// The lilypad's position relative to the pond window's top-left, in logical
/// px. Stored when the jump starts so the drop lands correctly even if the pond
/// window is moved while it is open.
static POND_PAD_OFFSET: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// Payload of `pond-hidden`: `drop` tells the overlay to fall from the pad
/// instead of simply reappearing at its old desktop spot.
#[derive(Clone, serde::Serialize)]
struct PondHidden {
    drop: bool,
}

/// Asks the real frog window to turn and jump onto the pond. Rust relays the
/// event so the request does not depend on the pond's webview emitting events.
/// The request is also stored so the overlay can poll for it: events are not
/// replayed to a webview that was asleep when they were emitted.
#[tauri::command]
fn start_pond_jump(app: AppHandle, request: JumpRequest) {
    POND_FROG_ON_PAD.store(false, std::sync::atomic::Ordering::Release);
    // Remember where the pad sits inside the pond (logical px). The pond can be
    // dragged while open, so the drop on close recomputes from its new origin.
    let pad_offset = app.get_webview_window(POND_LABEL).and_then(|pond| {
        let origin = pond.outer_position().ok()?;
        let scale = pond.scale_factor().ok().unwrap_or(1.0);
        Some((
            (request.target_x - origin.x as f64) / scale,
            (request.target_y - origin.y as f64) / scale,
        ))
    });
    if let Ok(mut offset) = POND_PAD_OFFSET.lock() {
        *offset = pad_offset;
    }
    if let Ok(mut pending) = POND_JUMP.lock() {
        *pending = Some(request.clone());
    }
    let seq = POND_JUMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
    let _ = app.emit("pond-jump", PendingJump { seq, request });
}

/// The most recent jump request, for the overlay's polling fallback. Only
/// answers while the pond is visible, so a reloaded overlay cannot replay an
/// old request and hide the frog on the desktop.
#[tauri::command]
fn pending_pond_jump(app: AppHandle) -> Option<PendingJump> {
    let pond = app.get_webview_window(POND_LABEL)?;
    if !pond.is_visible().unwrap_or(false) {
        return None;
    }
    let seq = POND_JUMP_SEQ.load(std::sync::atomic::Ordering::Acquire);
    if seq == 0 {
        return None;
    }
    POND_JUMP
        .lock()
        .ok()
        .and_then(|pending| pending.clone())
        .map(|request| PendingJump { seq, request })
}

/// The frog has landed; tell the pond to take over with its own sprite.
#[tauri::command]
fn finish_pond_jump(app: AppHandle) {
    POND_FROG_ON_PAD.store(true, std::sync::atomic::Ordering::Release);
    let _ = app.emit("pond-jump-done", ());
}

/// Called by the pond after it has let the frog sit on the desktop for a beat.
/// Only hides him when the pond is actually visible, so a close during the
/// delay cannot leave the frog invisible.
#[tauri::command]
fn hide_frog_overlay(app: AppHandle) {
    if let Some(pond) = app.get_webview_window(POND_LABEL) {
        if pond.is_visible().unwrap_or(false) {
            set_main_visible(&app, false);
        }
    }
}

/// Closes the pond and brings the frog back. When he had landed on the pad, put
/// the overlay back on the lilypad first: the frontend then drops him from there
/// with the same physics as a mouse release. The pad position is recomputed from
/// the pond's *current* window position, so moving the pond window before
/// closing still drops him from the pad's new screen spot.
fn close_pond(app: &AppHandle) {
    let pond = app.get_webview_window(POND_LABEL);
    let landing = if POND_FROG_ON_PAD.swap(false, std::sync::atomic::Ordering::AcqRel) {
        pond.as_ref().and_then(|pond| {
            let origin = pond.outer_position().ok()?;
            let scale = pond.scale_factor().ok().unwrap_or(1.0);
            let (pad_x, pad_y) = POND_PAD_OFFSET.lock().ok().and_then(|offset| *offset)?;
            Some(PhysicalPosition::new(
                (origin.x as f64 + pad_x * scale).round() as i32,
                (origin.y as f64 + pad_y * scale).round() as i32,
            ))
        })
    } else {
        None
    };
    if let Some(pond) = pond {
        let _ = pond.hide();
    }
    let mut drop = false;
    if let (Some(main), Some(spot)) = (app.get_webview_window("main"), landing) {
        let _ = main.set_position(spot);
        drop = true;
    }
    set_main_visible(app, true);
    let _ = app.emit("pond-hidden", PondHidden { drop });
}

/// Places the trigger remote just left of the frog, clamped to the work area,
/// so both stay visible together.
fn position_remote_beside_frog(app: &AppHandle, remote: &WebviewWindow) {
    let Ok(size) = remote.outer_size() else {
        return;
    };
    let mut x = 0;
    let mut y = 0;
    if let Some(main) = app.get_webview_window("main") {
        if let (Ok(position), Ok(main_size)) = (main.outer_position(), main.outer_size()) {
            x = position.x - size.width as i32 - 12;
            y = position.y + main_size.height as i32 - size.height as i32;
        }
    }
    if let Ok(Some(monitor)) = remote.current_monitor() {
        let area = monitor.work_area();
        let min_x = area.position.x + SCREEN_MARGIN;
        let min_y = area.position.y + SCREEN_MARGIN;
        let max_x = area.position.x + area.size.width as i32 - size.width as i32 - SCREEN_MARGIN;
        let max_y = area.position.y + area.size.height as i32 - size.height as i32 - SCREEN_MARGIN;
        if max_x > min_x {
            x = x.clamp(min_x, max_x);
        }
        if max_y > min_y {
            y = y.clamp(min_y, max_y);
        }
    }
    let _ = remote.set_position(PhysicalPosition::new(x, y));
}

/// Shows the small trigger remote beside the frog. The frog stays visible; the
/// remote is non-modal and has its own close button.
#[tauri::command]
fn open_remote(app: AppHandle) {
    let Some(remote) = app.get_webview_window(REMOTE_LABEL) else {
        return;
    };
    position_remote_beside_frog(&app, &remote);
    let _ = remote.show();
    let _ = remote.set_focus();
}

/// Testing-tab command: fire one detector with simulated input above its
/// threshold, bypassing the intervention cooldown.
#[tauri::command]
async fn demo_trigger(app: AppHandle, trigger: String) -> Result<(), String> {
    ai_engine::trigger_demo(app, trigger).await.map(|_| ())
}

/// Closes the pond and brings the frog back to the desktop.
#[tauri::command]
fn hide_pond(app: AppHandle) {
    close_pond(&app);
}

/// Opens the macOS Accessibility pane so the user can grant the selection
/// watcher permission to read highlighted text.
#[tauri::command]
fn open_accessibility_settings() -> Result<(), String> {
    tauri_plugin_opener::open_url(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        None::<&str>,
    )
    .map_err(|error| error.to_string())
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            chat::frog_chat,
            chat::summarize_selection,
            open_accessibility_settings,
            get_settings,
            set_nudges_paused,
            set_sound_enabled,
            set_sound_volume,
            set_mode,
            set_frog_color,
            set_selection_summaries,
            set_trigger_enabled,
            set_activity_enabled,
            quit_app,
            frog_stats,
            frog_anchor,
            open_pond,
            hide_pond,
            hide_frog_overlay,
            start_pond_jump,
            pending_pond_jump,
            finish_pond_jump,
            open_remote,
            demo_trigger,
            pond_tab
        ])
        .setup(|app| {
            // macOS: live only in the menu bar — no Dock tile, no Cmd-Tab entry.
            // Windows keeps both windows out of the taskbar via `skipTaskbar`.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            setup_tray(app)?;
            telemetry::start(app.handle().clone());
            window_tracker::start();
            ai_engine::start(app.handle().clone());

            if let Some(window) = app.get_webview_window("main") {
                // macOS: keep the frog visible when swiping between Spaces
                // and above other apps' windows on every Space.
                // Note: `alwaysOnTop` / `visibleOnAllWorkspaces` must stay out of
                // tauri.conf.json — tao applies them at window creation, which
                // pins the window to the desktop Space and makes it invisible
                // over another app's full-screen Space. Setting the level and
                // collection behavior here (after creation) joins all Spaces,
                // including full-screen ones.
                window.set_visible_on_all_workspaces(true)?;
                #[cfg(target_os = "macos")]
                float_over_fullscreen_spaces(&window)?;
                // Windows/Linux: the config can no longer carry `alwaysOnTop`
                // (tao applies it at creation and breaks macOS Spaces), so the
                // overlay has to be raised here or other windows cover the frog.
                #[cfg(not(target_os = "macos"))]
                window.set_always_on_top(true)?;
                position_bottom_right(&window)?;
            }
            selection::spawn(app.handle().clone());
            if let Some(panel) = app.get_webview_window("panel") {
                #[cfg(target_os = "macos")]
                float_over_fullscreen_spaces(&panel)?;
                // Setting the level/behavior above can order the panel in on
                // some macOS versions; keep it hidden until the tray toggles it.
                panel.hide()?;
            }
            if let Some(pond) = app.get_webview_window(POND_LABEL) {
                #[cfg(target_os = "macos")]
                float_over_fullscreen_spaces(&pond)?;
                pond.hide()?;
                // A normal decorated window: the title-bar X should hide the
                // pond and bring the frog back, not destroy the webview.
                let pond_handle = app.handle().clone();
                pond.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        close_pond(&pond_handle);
                    }
                });
            }
            // Dev/demo hooks: open the small popover or the big pond at launch.
            if std::env::var("FROG_OPEN_PANEL").is_ok() {
                toggle_panel(app.handle(), None);
            }
            if std::env::var("FROG_OPEN_POND").is_ok() {
                open_pond(app.handle().clone(), None);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
