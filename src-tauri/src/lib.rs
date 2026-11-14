mod ai_engine;
mod telemetry;
mod window_tracker;

use tauri::{Manager, PhysicalPosition, WebviewWindow};

/// Gap between the frog and the screen edges, in physical pixels.
const SCREEN_MARGIN: i32 = 12;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            telemetry::start(app.handle().clone());
            window_tracker::start();
            ai_engine::start(app.handle().clone());

            if let Some(window) = app.get_webview_window("main") {
                // macOS: keep the frog visible when swiping between Spaces
                // and above other apps' windows on every Space.
                // Note: no `set_always_on_top` here — tao applies it asynchronously
                // on the main queue, which would later clobber the status window
                // level set below with the lower floating level.
                window.set_visible_on_all_workspaces(true)?;
                #[cfg(target_os = "macos")]
                float_over_fullscreen_spaces(&window)?;
                position_bottom_right(&window)?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
