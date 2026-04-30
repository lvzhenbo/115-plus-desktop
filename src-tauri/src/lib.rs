//! 115+ 桌面客户端 — Tauri 应用入口。
//!
//! # 启动流程
//!
//! ```text
//! main.rs → lib::run()
//!   ├─ 注册插件（日志 / 更新 / 单实例 / 窗口状态 / …）
//!   ├─ on_window_event  → 关闭按钮 → 隐藏到托盘
//!   ├─ setup
//!   │   ├─ bind_log_level_to_setting_store  → 同步前端日志等级
//!   │   ├─ 扩展 asset scope → macOS/Linux 系统字体目录
//!   │   ├─ upload::init / download::init    → 业务模块初始化
//!   │   └─ tray::create                     → 系统托盘
//!   ├─ invoke_handler → 注册所有 Tauri command
//!   └─ run 事件循环
//!       └─ ExitRequested(code=None) → Cmd+Q / Alt+F4 → 阻止退出
//!           ExitRequested(code=0)   → 前端 exit(0)   → 放行
//! ```
//!
//! # 模块职责
//!
//! | 模块 | 职责 |
//! |------|------|
//! | `tray`     | 系统托盘图标、右键菜单、点击事件 |
//! | `download` | HTTP 多分片并发下载、断点续传、限速 |
//! | `upload`   | 115 网盘 OSS 上传、分片、队列调度 |
//! | `subtitle` | 系统字体扫描、ASS 字幕字体匹配 |

use chrono::Local;
use serde::Deserialize;
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
use tauri_plugin_pinia::ManagerExt as PiniaManagerExt;
use tauri_plugin_window_state::StateFlags;

mod download;
mod subtitle;
mod tray;
mod upload;

/// 日志等级枚举，反序列化自前端 `generalSetting.logLevel`。
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

/// Pinia `setting` store 中 `generalSetting` 的 Rust 投影。
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

/// 从 Pinia 同步日志等级到 Rust 日志系统。
///
/// 启动时读取初始值，之后 watch 前端变更，实现运行时热切换而不需重启。
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
        // ---- 插件注册 ----
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
                .max_file_size(50_000_000)
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
        .plugin(
            tauri_plugin_single_instance::Builder::new()
                .callback(|app, _args, _cwd| {
                    if let Err(err) = show_window(app) {
                        log::warn!("单实例唤醒主窗口失败：{}", err);
                    }
                })
                .dbus_id("io.github.lvzhenbo.oof-plus-desktop")
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_pinia::init())
        // ---- 窗口行为 ----
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                window.hide().ok();
                api.prevent_close(); // 关闭 → 隐藏到托盘，不退出
            }
        })
        // ---- 应用初始化 ----
        .setup(|app| {
            bind_log_level_to_setting_store(app)?;
            log::info!("应用启动，版本={}", app.package_info().version);

            // macOS / Linux：允许前端通过 asset 协议加载系统字体
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            if let Some(home) = std::env::var_os("HOME") {
                let scope = app.asset_protocol_scope();
                let home_path = std::path::Path::new(&home);

                #[cfg(target_os = "macos")]
                let _ = scope.allow_directory(home_path.join("Library/Fonts"), true);

                #[cfg(target_os = "linux")]
                {
                    let _ = scope.allow_directory(home_path.join(".local/share/fonts"), true);
                    let _ = scope.allow_directory(home_path.join(".fonts"), true);
                }
            }

            upload::init(app).map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })?;
            download::init(app).map_err(|err| -> Box<dyn std::error::Error> { Box::new(err) })?;
            tray::create(app.handle())?;

            log::info!("应用初始化完成");
            Ok(())
        })
        // ---- Tauri command 注册 ----
        .invoke_handler(tauri::generate_handler![
            subtitle::subtitle_get_system_font_config,
            // 上传
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
            // 下载
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
        // ---- 运行 ----
        .build(tauri::generate_context!())
        .unwrap_or_else(|err| panic!("Tauri 应用运行失败：{}", err))
        .run(|_app, event| {
            if let RunEvent::ExitRequested { code, api, .. } = event {
                // code=None  → 系统快捷键（Cmd+Q / Alt+F4）→ 阻止，窗口已隐藏
                // code=0     → 前端 exit(0) → 托盘确认退出 → 放行
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

/// 展示并聚焦主窗口（托盘点击、单实例唤醒等场景复用）。
pub(crate) fn show_window(app: &AppHandle) -> Result<(), String> {
    let windows = app.webview_windows();
    let window = windows
        .values()
        .next()
        .ok_or_else(|| "未找到可用的主窗口".to_string())?;

    if window.is_minimized().unwrap_or(false) {
        window
            .unminimize()
            .map_err(|err| format!("恢复最小化窗口失败：{}", err))?;
    }
    window
        .show()
        .map_err(|err| format!("显示主窗口失败：{}", err))?;
    window
        .set_focus()
        .map_err(|err| format!("聚焦主窗口失败：{}", err))?;

    Ok(())
}
