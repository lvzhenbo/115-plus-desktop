pub mod events;
pub mod http;
pub mod persistence;
pub mod queue;
pub mod segment;
pub mod store;
pub mod throttle;
pub mod types;
pub mod writer;

use events::EventBridge;
use persistence::ProgressFile;
use queue::TaskQueue;
use std::sync::Arc;
use store::DbHandle;
use tauri::{App, Manager};

#[derive(Debug, thiserror::Error)]
pub enum DownloadInitError {
    #[error("无法构建下载 HTTP 客户端：{0}")]
    BuildHttpClient(#[from] reqwest::Error),
    #[error("无法解析应用数据目录：{0}")]
    ResolveAppDataDir(String),
    #[error("无法创建应用数据目录 {path}：{source}")]
    CreateAppDataDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("无法初始化下载数据库：{0}")]
    InitDatabase(#[from] store::DmError),
}

/// 下载模块初始化 — 创建所有依赖并注册为 Tauri managed state
///
/// 初始化顺序：ProgressFile → HTTP Client → DbHandle → FolderAggregator → EventBridge → TaskQueue
pub fn init(app: &App) -> Result<(), DownloadInitError> {
    // 1. .oofp 进度文件管理器
    let progress_file = Arc::new(ProgressFile::new());
    let progress_file_for_queue = progress_file.clone();
    app.manage(progress_file);

    // 2. 全局 HTTP 客户端（连接池 + HTTP/2 多路复用）
    let http_client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()?;
    let http_client_for_queue = http_client.clone();
    app.manage(http_client);

    // 3. 下载任务 DB Actor
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| DownloadInitError::ResolveAppDataDir(err.to_string()))?;
    std::fs::create_dir_all(&app_data_dir).map_err(|source| {
        DownloadInitError::CreateAppDataDir {
            path: app_data_dir.display().to_string(),
            source,
        }
    })?;
    let db_path = app_data_dir.join("downloads.db");
    let db_handle = DbHandle::new(db_path.to_string_lossy().to_string())?;
    let db_for_events = db_handle.clone();
    let db_for_queue = db_handle.clone();
    app.manage(db_handle);

    // 4. 文件夹进度聚合器
    let folder_aggregator = Arc::new(events::FolderAggregator::new());
    let folder_aggregator_for_queue = folder_aggregator.clone();
    app.manage(folder_aggregator.clone());

    // 5. 事件桥接（state-sync 去抖 + 进度聚合 + URL 解析）
    let event_bridge = EventBridge::start(app.handle().clone(), db_for_events, folder_aggregator);
    let state_sync_notify = event_bridge.state_sync_notify.clone();
    let url_resolver_for_queue = event_bridge.url_resolver.clone();
    let progress_registry = event_bridge.progress_registry.clone();
    app.manage(event_bridge.url_resolver.clone());
    app.manage(event_bridge);

    // 6. 下载队列调度器
    let task_queue = TaskQueue::start(
        app.handle().clone(),
        db_for_queue,
        state_sync_notify,
        url_resolver_for_queue,
        progress_registry,
        http_client_for_queue,
        progress_file_for_queue,
        folder_aggregator_for_queue,
    );
    app.manage(task_queue);

    Ok(())
}
