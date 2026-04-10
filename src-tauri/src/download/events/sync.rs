use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use tokio::time::{Duration, sleep};

use super::super::store::DbHandle;

/// 状态同步去抖循环。
///
/// 收到通知后等待 150ms，再查询数据库并发射 download:state-sync，避免短时间内重复刷新。
pub async fn state_sync_loop(notify: Arc<Notify>, app: AppHandle, db: DbHandle) {
    loop {
        notify.notified().await;
        // 150ms 去抖窗口，在实时性和事件风暴之间取平衡。
        sleep(Duration::from_millis(150)).await;
        match db.get_top_level_tasks().await {
            Ok(tasks) => {
                let _ = app.emit("download:state-sync", &tasks);
            }
            Err(e) => log::error!("状态同步查询失败: {}", e),
        }
    }
}

/// 主动推送单个任务状态。
///
/// 用于弥补批量 state-sync 的去抖延迟，让前端更快拿到关键状态变化。
pub async fn emit_download_task_status(app: &AppHandle, db: &DbHandle, gid: &str) {
    if let Ok(Some(task)) = db.get_task_by_gid(gid.to_string()).await {
        let _ = app.emit("download:task-status", &task);
    }
}
