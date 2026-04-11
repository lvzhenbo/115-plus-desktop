//! 上传状态同步层。
//!
//! 这一层只负责把后端状态变化节流后同步给前端，避免把事件推送逻辑混进存储层或调度层。

use std::sync::Arc;

use log::{debug, error, info, warn};
use tauri::{App, AppHandle, Emitter, Manager};
use tokio::sync::Notify;
use tokio::time::{Duration, sleep};

use super::store::DbHandle;

/// 前端状态同步触发器。
///
/// 实际状态仍然以数据库查询结果为准，这里只负责告诉后台“该重新拉一次列表了”。
#[derive(Clone)]
pub struct UploadStateSync {
    notify: Arc<Notify>,
}

impl UploadStateSync {
    /// 触发一次状态同步。
    pub fn notify_state_change(&self) {
        debug!("[上传状态同步] 收到状态变更通知");
        self.notify.notify_one();
    }
}

/// 监听状态变化并向前端推送最新顶层任务列表。
///
/// 150ms 的短暂合并窗口用来把连续写库产生的多次刷新合并成一次事件。
async fn state_sync_loop(notify: Arc<Notify>, app: AppHandle, db: DbHandle) {
    loop {
        notify.notified().await;
        sleep(Duration::from_millis(150)).await;
        match db.get_top_level_tasks().await {
            Ok(tasks) => {
                match app.emit("upload:state-sync", &tasks) {
                    Ok(()) => debug!("[上传状态同步] 已推送顶层任务 count={}", tasks.len()),
                    Err(err) => warn!(
                        "[上传状态同步] 推送顶层任务失败 count={}: {}",
                        tasks.len(),
                        err
                    ),
                }
            }
            Err(err) => error!("[上传状态同步] 查询顶层任务失败: {}", err),
        }
    }
}

/// 初始化上传状态同步器，并注册进 Tauri 全局状态。
pub fn init(app: &App) {
    let db = app.state::<DbHandle>().inner().clone();
    let state_sync = UploadStateSync {
        notify: Arc::new(Notify::new()),
    };

    tauri::async_runtime::spawn(state_sync_loop(
        state_sync.notify.clone(),
        app.handle().clone(),
        db,
    ));

    app.manage(state_sync);
    info!("[上传状态同步] 同步器已初始化");
}
