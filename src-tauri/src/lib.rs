use chrono::Local;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
use tauri_plugin_window_state::StateFlags;

mod database;
mod download;
mod upload;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_file_name = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir {
                        file_name: Some(log_file_name),
                    }),
                    Target::new(TargetKind::Webview),
                ])
                .rotation_strategy(RotationStrategy::KeepAll)
                .max_file_size(50_000_000) // 50MB
                .level(log::LevelFilter::Info)
                .timezone_strategy(TimezoneStrategy::UseLocal)
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
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
            log::info!("应用启动, 版本={}", app.package_info().version);

            // Initialize .oofp progress file manager
            let progress_file = Arc::new(download::persistence::ProgressFile::new());
            app.manage(progress_file);

            // Create global HTTP client for downloads (connection pooling + HTTP/2 multiplexing)
            let http_client = reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client");
            app.manage(http_client);

            log::info!("应用初始化完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            upload::compute_file_hash,
            upload::compute_partial_sha1,
            upload::upload_to_oss,
            upload::pause_upload,
            upload::cancel_upload,
            upload::resume_upload,
            upload::scan_directory,
            upload::get_file_size,
            download::http::pause_download,
            download::http::cancel_download,
            download::http::update_download_url,
            download::http::start_download,
            download::http::resume_download_task,
            download::http::set_speed_limit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn show_window(app: &AppHandle) {
    let windows = app.webview_windows();

    windows
        .values()
        .next()
        .expect("Sorry, no window found")
        .set_focus()
        .expect("Can't Bring Window to Focus");
}
