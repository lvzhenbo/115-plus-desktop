use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tauri_plugin_pinia::ManagerExt;
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandChild;

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
pub fn start_aria2_service(app: &AppHandle) -> Result<(), String> {
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

    // 尝试从 pinia store 读取用户保存的并行下载数设置
    let max_concurrent = app
        .pinia()
        .get::<serde_json::Value>("setting", "downloadSetting")
        .ok()
        .and_then(|v| v.get("maxConcurrent").and_then(|v| v.as_u64()))
        .unwrap_or(5);

    // 使用 sidecar 功能启动 aria2c
    // 不使用 session 文件，由前端自行管理下载列表和恢复
    let sidecar = app
        .shell()
        .sidecar("aria2c")
        .map_err(|e| format!("无法创建 aria2c sidecar: {}", e))?;

    let max_concurrent_arg = format!("--max-concurrent-downloads={}", max_concurrent);
    let command_child = sidecar
        .args([
            "--continue",
            "--enable-rpc",
            &format!("--rpc-listen-port={}", port),
            "--rpc-allow-origin-all",
            "--rpc-listen-all",
            "--daemon=false",
            &max_concurrent_arg,
        ])
        .spawn()
        .map_err(|e| format!("无法启动 aria2c: {}", e))?;

    // 存储进程以便之后可以关闭它
    *process = Some(command_child.1);

    println!("aria2c RPC 服务已启动在端口 {}", port);
    Ok(())
}

// 停止 aria2c 服务
pub fn stop_aria2_service() -> Result<(), String> {
    let mut process = ARIA2_PROCESS.lock().unwrap();
    if let Some(child) = process.take() {
        child
            .kill()
            .map_err(|e| format!("无法终止 aria2c 进程: {}", e))?;
        println!("aria2c RPC 服务已关闭");
    }
    Ok(())
}

#[tauri::command]
pub fn get_port() -> u16 {
    let port = ARIA2_PORT.lock().unwrap();
    *port
}

#[tauri::command]
pub fn stop_aria2() -> Result<(), String> {
    stop_aria2_service()
}
