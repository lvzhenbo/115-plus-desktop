//! Tauri 应用入口。
//!
//! 这个文件只负责应用级初始化：
//! - 安装通用插件
//! - 绑定设置中心里的日志级别
//! - 初始化上传/下载后端模块
//! - 注册前端可调用的 Tauri command

use chrono::Local;
use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
use tauri_plugin_pinia::ManagerExt as PiniaManagerExt;
use tauri_plugin_window_state::StateFlags;

mod download;
mod upload;

/// 与前端设置项 `generalSetting.logLevel` 对应的日志级别枚举。
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

/// 统一设置当前进程的最大日志等级。
fn set_log_level(level: AppLogLevel) {
    log::set_max_level(level.into());
}

/// 监听 Pinia 设置仓库中的日志级别变化，并实时同步到 Rust 日志系统。
///
/// 这里在应用启动时先读取一次初始值，再注册 watch，保证运行中的日志输出
/// 能跟随前端设置即时调整，而不需要重启应用。
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
    // 按启动时间切分日志文件，避免多次启动混写同一份文件。
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
        .setup(|app| {
            // 先绑定通用能力，再初始化业务模块，保证模块初始化期间的日志也受设置控制。
            bind_log_level_to_setting_store(app)?;
            log::info!("应用启动，版本={}", app.package_info().version);
            upload::init(app).map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })?;
            download::init(app).map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })?;
            log::info!("应用初始化完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 上传模块：前端必要的本地能力、队列控制与主列表查询。
            upload::local::upload_get_file_size,
            upload::api::upload_provide_api_response,
            upload::api::upload_provide_api_error,
            upload::queue::upload_set_max_concurrent,
            upload::queue::upload_set_max_retry,
            upload::queue::upload_enqueue_files,
            upload::queue::upload_enqueue_folder,
            upload::queue::upload_pause_task,
            upload::queue::upload_resume_task,
            upload::queue::upload_retry_task,
            upload::queue::upload_remove_task,
            upload::queue::upload_pause_folder,
            upload::queue::upload_resume_folder,
            upload::queue::upload_retry_folder,
            upload::queue::upload_remove_folder,
            upload::queue::upload_pause_all,
            upload::queue::upload_resume_all,
            upload::store::upload_delete_finished_tasks,
            upload::store::upload_get_top_level_tasks,
            // 下载模块：主列表查询、事件桥接和调度控制。
            download::store::download_delete_finished_tasks,
            download::store::download_get_top_level_tasks,
            download::events::download_provide_url,
            download::queue::download_enqueue_file,
            download::queue::download_set_max_concurrent,
            download::queue::download_set_speed_limit,
            download::queue::download_pause_task,
            download::queue::download_cancel_task,
            download::queue::download_resume_task,
            download::queue::download_retry_task,
            download::queue::download_create_folder_task,
            download::queue::download_restart_folder_collection,
            download::queue::download_fail_folder_collection,
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

/// 在单实例唤醒或其他需要前台化窗口的场景下展示并聚焦主窗口。
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
