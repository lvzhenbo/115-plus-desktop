use chrono::Local;
use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
use tauri_plugin_pinia::ManagerExt as PiniaManagerExt;
use tauri_plugin_window_state::StateFlags;

mod database;
mod download;
mod upload;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum AppLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Default for AppLogLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl From<AppLogLevel> for log::LevelFilter {
    fn from(value: AppLogLevel) -> Self {
        match value {
            AppLogLevel::Trace => log::LevelFilter::Trace,
            AppLogLevel::Debug => log::LevelFilter::Debug,
            AppLogLevel::Info => log::LevelFilter::Info,
            AppLogLevel::Warn => log::LevelFilter::Warn,
            AppLogLevel::Error => log::LevelFilter::Error,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GeneralSettingState {
    #[serde(default)]
    log_level: AppLogLevel,
}

fn set_log_level(level: AppLogLevel) {
    log::set_max_level(level.into());
}

fn bind_log_level_to_setting_store<R: tauri::Runtime, M: Manager<R>>(
    manager: &M,
) -> tauri_plugin_pinia::Result<()> {
    manager.with_store("setting", |store| {
        let general_setting = store.get_or("generalSetting", GeneralSettingState::default());
        set_log_level(general_setting.log_level);

        store.watch(|app| {
            let general_setting =
                app.pinia()
                    .get_or("setting", "generalSetting", GeneralSettingState::default());
            set_log_level(general_setting.log_level);
            Ok(())
        });
    })?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(deprecated)]
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
                .level(log::LevelFilter::Trace)
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
            if let Err(err) = show_window(app) {
                log::warn!("单实例唤醒主窗口失败：{}", err);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_pinia::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:uploads.db", database::uploads_migrations())
                .build(),
        )
        .setup(|app| {
            bind_log_level_to_setting_store(app)?;
            log::info!("应用启动，版本={}", app.package_info().version);
            download::init(app).map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })?;
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
            download::store::download_insert_task,
            download::store::download_batch_insert_tasks,
            download::store::download_update_task,
            download::store::download_delete_task,
            download::store::download_delete_child_tasks,
            download::store::download_delete_finished_tasks,
            download::store::download_get_top_level_tasks,
            download::store::download_get_all_tasks,
            download::store::download_get_task_by_gid,
            download::store::download_get_child_tasks,
            download::store::download_get_incomplete_tasks,
            download::store::download_get_active_gids,
            download::store::download_has_active_tasks,
            download::store::download_get_download_stats,
            download::events::download_provide_url,
            download::queue::download_enqueue_file,
            download::queue::download_set_max_concurrent,
            download::queue::download_set_speed_limit,
            download::queue::download_pause_task,
            download::queue::download_cancel_task,
            download::queue::download_resume_task,
            download::queue::download_retry_task,
            download::queue::download_enqueue_folder,
            download::queue::download_pause_folder,
            download::queue::download_resume_folder,
            download::queue::download_cancel_folder,
            download::queue::download_retry_folder,
            download::queue::download_pause_all,
            download::queue::download_resume_all,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| panic!("Tauri 应用运行失败：{}", err));
}

fn show_window(app: &AppHandle) -> Result<(), String> {
    let windows = app.webview_windows();
    let window = windows
        .values()
        .next()
        .ok_or_else(|| "未找到可用的主窗口".to_string())?;

    window
        .show()
        .map_err(|err| format!("显示主窗口失败：{}", err))?;
    window
        .set_focus()
        .map_err(|err| format!("聚焦主窗口失败：{}", err))?;

    Ok(())
}
