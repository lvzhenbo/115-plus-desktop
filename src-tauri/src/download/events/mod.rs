pub mod folder;
pub mod progress;
pub mod sync;
pub mod url;

// 重新导出公共 API，兼容旧模块引用路径。
pub use folder::FolderAggregator;
pub use progress::{
    DownloadProgressEvent, DownloadSegmentEvent, DownloadTaskEvent, ProgressItem, ProgressRegistry,
    SpeedCalculator, UrlExpiredEvent, emit_progress, emit_segment_status, emit_task_status,
    emit_url_expired,
};
pub use sync::emit_download_task_status;
#[doc(hidden)]
pub use url::__cmd__download_provide_url;
pub use url::UrlResolver;
pub use url::download_provide_url;

use super::store::DbHandle;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Notify;

/// 下载事件基础设施。
pub struct EventBridge {
    pub state_sync_notify: Arc<Notify>,
    pub progress_registry: Arc<ProgressRegistry>,
    pub url_resolver: Arc<UrlResolver>,
}

impl EventBridge {
    pub fn start(app: AppHandle, db: DbHandle, folder_aggregator: Arc<FolderAggregator>) -> Self {
        let notify = Arc::new(Notify::new());
        let registry = Arc::new(ProgressRegistry::new());
        let resolver = Arc::new(UrlResolver::new());

        // 启动状态同步去抖循环。
        {
            let n = notify.clone();
            let a = app.clone();
            let d = db.clone();
            tauri::async_runtime::spawn(async move {
                sync::state_sync_loop(n, a, d).await;
            });
        }

        // 启动进度聚合循环。
        {
            let r = registry.clone();
            let fa = folder_aggregator.clone();
            let a = app.clone();
            tauri::async_runtime::spawn(async move {
                progress::progress_loop(r, fa, a).await;
            });
        }

        Self {
            state_sync_notify: notify,
            progress_registry: registry,
            url_resolver: resolver,
        }
    }

    pub fn notify_state_change(&self) {
        self.state_sync_notify.notify_one();
    }
}
