mod ai_engine;
mod telemetry;
mod window_tracker;

use tauri::{Manager, PhysicalPosition, WebviewWindow};

/// Gap between the frog and the screen edges, in physical pixels.
const SCREEN_MARGIN: i32 = 12;

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
                position_bottom_right(&window)?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
