use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_sql::{Migration, MigrationKind};

use tauri_plugin_window_state::StateFlags;

mod upload;

// 存储 aria2c 进程的全局变量
lazy_static::lazy_static! {
    static ref ARIA2_PROCESS: Arc<Mutex<Option<CommandChild>>> = Arc::new(Mutex::new(None));
    static ref ARIA2_PORT: Arc<Mutex<u16>> = Arc::new(Mutex::new(6800)); // 默认端口6800
}

// 检查端口是否可用
fn is_port_available(port: u16) -> bool {
    TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok()
}

// 查找可用端口
fn find_available_port(start_port: u16) -> Option<u16> {
    let mut port = start_port;
    // 尝试从start_port开始找到一个可用的端口，最多尝试100个端口
    for _ in 0..100 {
        if is_port_available(port) {
            return Some(port);
        }
        port += 1;
    }
    None
}

// 启动 aria2c RPC 服务
fn start_aria2_service(app: &AppHandle) -> Result<(), String> {
    // 确保服务没有重复启动
    let mut process = ARIA2_PROCESS.lock().unwrap();
    if process.is_some() {
        return Ok(());
    }

    // 查找可用端口
    let default_port = 6800;
    let port = find_available_port(default_port).ok_or_else(|| "没有找到可用的端口".to_string())?;

    // 更新全局端口变量
    {
        let mut aria2_port = ARIA2_PORT.lock().unwrap();
        *aria2_port = port;
    }

    // 使用 sidecar 功能启动 aria2c
    // 不使用 session 文件，由前端自行管理下载列表和恢复
    let sidecar = app
        .shell()
        .sidecar("aria2c")
        .map_err(|e| format!("无法创建 aria2c sidecar: {}", e))?;

    let command_child = sidecar
        .args([
            "--continue",
            "--enable-rpc",
            &format!("--rpc-listen-port={}", port),
            "--rpc-allow-origin-all",
            "--rpc-listen-all",
            "--daemon=false",
        ])
        .spawn()
        .map_err(|e| format!("无法启动 aria2c: {}", e))?;

    // 存储进程以便之后可以关闭它
    *process = Some(command_child.1);

    println!("aria2c RPC 服务已启动在端口 {}", port);
    Ok(())
}

// 停止 aria2c 服务
fn stop_aria2_service() -> Result<(), String> {
    let mut process = ARIA2_PROCESS.lock().unwrap();
    if let Some(child) = process.take() {
        child
            .kill()
            .map_err(|e| format!("无法终止 aria2c 进程: {}", e))?;
        println!("aria2c RPC 服务已关闭");
    }
    Ok(())
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn get_port() -> u16 {
    let port = ARIA2_PORT.lock().unwrap();
    *port
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
                .add_migrations(
                    "sqlite:downloads.db",
                    vec![Migration {
                        version: 1,
                        description: "create_downloads_table",
                        sql: "CREATE TABLE IF NOT EXISTS downloads (
                            gid TEXT PRIMARY KEY,
                            fid TEXT NOT NULL,
                            name TEXT NOT NULL,
                            pick_code TEXT NOT NULL,
                            size INTEGER NOT NULL DEFAULT 0,
                            status TEXT NOT NULL DEFAULT 'active',
                            progress REAL NOT NULL DEFAULT 0,
                            path TEXT,
                            download_speed INTEGER NOT NULL DEFAULT 0,
                            eta INTEGER,
                            error_message TEXT,
                            error_code TEXT,
                            created_at INTEGER,
                            completed_at INTEGER,
                            is_folder INTEGER NOT NULL DEFAULT 0,
                            is_collecting INTEGER NOT NULL DEFAULT 0,
                            parent_gid TEXT,
                            total_files INTEGER,
                            completed_files INTEGER,
                            failed_files INTEGER
                        );",
                        kind: MigrationKind::Up,
                    }],
                )
                .add_migrations(
                    "sqlite:uploads.db",
                    vec![Migration {
                        version: 1,
                        description: "create_uploads_table",
                        sql: "CREATE TABLE IF NOT EXISTS uploads (
                            id TEXT PRIMARY KEY,
                            file_name TEXT NOT NULL,
                            file_path TEXT NOT NULL,
                            file_size INTEGER NOT NULL DEFAULT 0,
                            target_cid TEXT NOT NULL DEFAULT '0',
                            target_path TEXT,
                            sha1 TEXT,
                            pre_sha1 TEXT,
                            pick_code TEXT,
                            status TEXT NOT NULL DEFAULT 'pending',
                            progress REAL NOT NULL DEFAULT 0,
                            upload_speed INTEGER NOT NULL DEFAULT 0,
                            error_message TEXT,
                            created_at INTEGER,
                            completed_at INTEGER,
                            is_folder INTEGER NOT NULL DEFAULT 0,
                            parent_id TEXT,
                            total_files INTEGER,
                            completed_files INTEGER,
                            failed_files INTEGER,
                            oss_bucket TEXT,
                            oss_object TEXT,
                            oss_endpoint TEXT,
                            callback TEXT,
                            callback_var TEXT,
                            uploaded_size INTEGER NOT NULL DEFAULT 0,
                            file_id TEXT,
                            oss_upload_id TEXT
                        );",
                        kind: MigrationKind::Up,
                    }],
                )
                .build(),
        )
        .setup(|app| {
            // 应用启动时启动 aria2c
            if let Err(e) = start_aria2_service(app.handle()) {
                eprintln!("启动 aria2c 服务失败: {}", e);
            }
            Ok(())
        })
        .on_window_event(|app_handle, event| {
            // 当最后一个窗口关闭时，关闭 aria2c
            if let tauri::WindowEvent::Destroyed = event {
                if app_handle.webview_windows().len() < 1 {
                    if let Err(e) = stop_aria2_service() {
                        eprintln!("关闭 aria2c 服务失败: {}", e);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_port,
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

    windows
        .values()
        .next()
        .expect("Sorry, no window found")
        .set_focus()
        .expect("Can't Bring Window to Focus");
}
