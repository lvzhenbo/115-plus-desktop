use tauri::{AppHandle, Manager};
use tauri_plugin_window_state::StateFlags;

mod aria2;
mod database;
mod upload;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = show_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_pinia::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:downloads.db", database::downloads_migrations())
                .add_migrations("sqlite:uploads.db", database::uploads_migrations())
                .build(),
        )
        .setup(|app| {
            // 应用启动时启动 aria2c
            if let Err(e) = aria2::start_aria2_service(app.handle()) {
                eprintln!("启动 aria2c 服务失败: {}", e);
            }
            Ok(())
        })
        .on_window_event(|app_handle, event| {
            // 当最后一个窗口关闭时，关闭 aria2c
            if let tauri::WindowEvent::Destroyed = event {
                if app_handle.webview_windows().len() < 1 {
                    if let Err(e) = aria2::stop_aria2_service() {
                        eprintln!("关闭 aria2c 服务失败: {}", e);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            aria2::get_port,
            aria2::stop_aria2,
            upload::compute_file_hash,
            upload::compute_partial_sha1,
            upload::upload_to_oss,
            upload::pause_upload,
            upload::cancel_upload,
            upload::resume_upload,
            upload::scan_directory,
            upload::get_file_size
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn show_window(app: &AppHandle) {
    let windows = app.webview_windows();

    if let Some(window) = windows.values().next() {
        if let Err(e) = window.set_focus() {
            eprintln!("无法将窗口置于焦点: {}", e);
        }
    }
}
