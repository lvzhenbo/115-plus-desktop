use std::fs;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::ShellExt;

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

    // 获取config_dir路径并确保session文件存在
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法获取配置目录: {}", e))?;

    // 确保配置目录存在
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir).map_err(|e| format!("无法创建配置目录: {}", e))?;
    }

    // 创建session文件路径
    let session_file = config_dir.join("aria2.session");

    // 如果session文件不存在，创建一个空文件
    if !session_file.exists() {
        fs::write(&session_file, "").map_err(|e| format!("无法创建session文件: {}", e))?;
        println!("已创建aria2 session文件: {:?}", session_file);
    }

    // 使用 sidecar 功能启动 aria2c
    // 常用的 aria2c 参数：
    // --continue：继续下载
    // --enable-rpc：启动 RPC 服务
    // --rpc-listen-port=6800：指定 RPC 端口
    // --rpc-allow-origin-all：允许所有来源的请求
    // --rpc-listen-all：监听所有网络接口
    // --daemon=false：不作为守护进程运行（为了让 Tauri 能够管理进程）
    // --input-file：指定要加载的会话文件，恢复之前的下载
    // --save-session：指定退出时保存进行中下载的会话文件
    let sidecar = app
        .shell()
        .sidecar("aria2c")
        .map_err(|e| format!("无法创建 aria2c sidecar: {}", e))?;

    let session_file_str = session_file
        .to_str()
        .ok_or_else(|| "无法将session文件路径转换为字符串".to_string())?;

    let command_child = sidecar
        .args([
            "--continue",
            "--enable-rpc",
            &format!("--rpc-listen-port={}", port),
            "--rpc-allow-origin-all",
            "--rpc-listen-all",
            "--daemon=false",
            &format!("--input-file={}", session_file_str),
            &format!("--save-session={}", session_file_str),
            "--save-session-interval=60", // 每60秒自动保存一次会话
        ])
        .spawn()
        .map_err(|e| format!("无法启动 aria2c: {}", e))?;

    // 存储进程以便之后可以关闭它
    *process = Some(command_child.1);

    println!("aria2c RPC 服务已启动在端口 {}", port);
    println!("使用session文件: {}", session_file_str);
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
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = show_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
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
        .invoke_handler(tauri::generate_handler![get_port])
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
