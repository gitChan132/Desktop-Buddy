mod ai_engine;
mod chat;
mod settings;
mod telemetry;
mod window_tracker;

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
const SETTINGS_EVENT: &str = "frog-settings";

/// Holds the tray menu so the check marks can be synced from commands.
struct TrayMenu(tauri::menu::Menu<tauri::Wry>);

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

/// Anchors the frameless frog window to the bottom-right of the monitor work area.
fn position_bottom_right(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let size = window.outer_size()?;

    let x = area.position.x + area.size.width as i32 - size.width as i32 - SCREEN_MARGIN;
    let y = area.position.y + area.size.height as i32 - size.height as i32 - SCREEN_MARGIN;
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

/// Opens or hides the control panel, anchoring it near the clicked tray icon.
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
fn quit_app(app: AppHandle) {
    app.exit(0);
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            chat::frog_chat,
            get_settings,
            set_nudges_paused,
            set_sound_enabled,
            set_sound_volume,
            quit_app
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
                position_bottom_right(&window)?;
            }
            if let Some(panel) = app.get_webview_window("panel") {
                #[cfg(target_os = "macos")]
                float_over_fullscreen_spaces(&panel)?;
                // Setting the level/behavior above can order the panel in on
                // some macOS versions; keep it hidden until the tray toggles it.
                panel.hide()?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
