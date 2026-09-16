pub mod events;
pub mod http;
pub mod persistence;
pub mod queue;
pub mod segment;
pub mod store;
pub mod throttle;
pub mod types;
pub mod writer;

use crate::proxy::{ProxyDecision, ProxyFeature, ProxyState};
use events::EventBridge;
use persistence::ProgressFile;
use queue::TaskQueue;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use store::DbHandle;
use tauri::{App, Manager};

#[derive(Debug, thiserror::Error)]
pub enum DownloadInitError {
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

/// 按当前代理设置提供下载 HTTP 客户端。
///
/// 代理设置变化时惰性重建客户端；旧客户端若仍有任务持有会继续工作，直到引用耗尽后销毁。
pub struct DownloadHttpClient {
    proxy_state: Arc<ProxyState>,
    cached: Mutex<Option<(ProxyDecision, reqwest::Client)>>,
}

impl DownloadHttpClient {
    pub fn new(proxy_state: Arc<ProxyState>) -> Self {
        Self {
            proxy_state,
            cached: Mutex::new(None),
        }
    }

    /// 获取与当前代理设置匹配的客户端。
    pub fn client(&self) -> reqwest::Client {
        let decision = self.proxy_state.resolve(ProxyFeature::Download);

        {
            let cached = self.cached.lock().unwrap();
            if let Some((cached_decision, client)) = cached.as_ref()
                && *cached_decision == decision
            {
                return client.clone();
            }
        }

        let client = build_client(&decision);
        *self.cached.lock().unwrap() = Some((decision, client.clone()));
        client
    }
}

/// 按代理决策构建下载 HTTP 客户端。
fn build_client(decision: &ProxyDecision) -> reqwest::Client {
    log::info!(
        "[下载] 按当前代理设置构建 HTTP 客户端：{}",
        decision.describe()
    );

    let builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(30));
    let builder = match decision {
        ProxyDecision::Explicit { url } => match reqwest::Proxy::all(url) {
            Ok(proxy) => builder.proxy(proxy),
            Err(err) => {
                log::warn!("[下载] 代理地址解析失败，本次不使用代理：{url}（{err}）");
                builder.no_proxy()
            }
        },
        // 不显式设置代理时，reqwest 会自动使用系统代理。
        ProxyDecision::System => builder,
        ProxyDecision::Direct => builder.no_proxy(),
    };

    builder.build().unwrap_or_else(|err| {
        log::error!("[下载] 构建 HTTP 客户端失败，回退到默认客户端：{err}");
        reqwest::Client::new()
    })
}

/// 下载模块初始化 — 创建所有依赖并注册为 Tauri managed state
///
/// 初始化顺序：ProgressFile → HTTP Client → DbHandle → FolderAggregator → EventBridge → TaskQueue
pub fn init(app: &App) -> Result<(), DownloadInitError> {
    // 1. .oofp 进度文件管理器
    let progress_file = Arc::new(ProgressFile::new());
    let progress_file_for_queue = progress_file.clone();
    app.manage(progress_file);

    // 2. 下载 HTTP 客户端提供者（按代理设置惰性构建 / 重建）
    let proxy_state = app.state::<Arc<ProxyState>>().inner().clone();
    let http_client = Arc::new(DownloadHttpClient::new(proxy_state));
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

#[cfg(test)]
mod tests {
    use super::build_client;
    use crate::proxy::ProxyDecision;

    #[test]
    fn builds_download_clients_for_all_proxy_decisions() {
        // socks5 依赖 reqwest 的 socks feature，这里确保其可用。
        assert!(reqwest::Proxy::all("socks5://127.0.0.1:1080").is_ok());

        let _ = build_client(&ProxyDecision::Explicit {
            url: "socks5://127.0.0.1:1080".to_string(),
        });
        let _ = build_client(&ProxyDecision::System);
        let _ = build_client(&ProxyDecision::Direct);
    }
}
