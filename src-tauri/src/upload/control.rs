//! 上传运行时控制信号中心。
//!
//! 这里不做队列调度，也不改数据库；职责只有一个：
//! 给“已经在执行 OSS 上传”的任务维护暂停/恢复/取消信号，并暴露给调度器调用。

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use log::{info, warn};
use tokio::sync::watch;

use super::error::{UploadError, UploadResult, message_error};

/// 上传任务 ID 到控制信号发送端的映射表。
///
/// 只有正在执行的任务才会注册到这里；任务结束后会由 `oss.rs` 主动移除。
pub(crate) type UploadSignalMap = HashMap<String, watch::Sender<UploadSignal>>;

/// 全局上传信号注册表。
///
/// 上传执行器在启动时写入、结束时移除，控制命令通过它向运行中的任务广播状态变化。
static UPLOAD_SIGNALS: LazyLock<Arc<Mutex<UploadSignalMap>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

/// 运行中上传任务可感知的控制信号。
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UploadSignal {
    /// 继续正常执行。
    Running,
    /// 尽快中断当前流程并返回“已暂停”，由上层决定何时恢复。
    Paused,
    /// 终止任务并执行必要清理。
    Cancelled,
}

/// 安全获取全局上传信号注册表。
pub(crate) fn upload_signal_registry() -> UploadResult<MutexGuard<'static, UploadSignalMap>> {
    UPLOAD_SIGNALS
        .lock()
        .map_err(|_| UploadError::SignalRegistryPoisoned)
}

/// 向运行中的上传任务发送暂停信号。
pub(crate) fn upload_pause(upload_id: String) -> Result<(), String> {
    info!("[上传控制] 请求暂停 id={}", upload_id);
    let signals = upload_signal_registry().map_err(|err| err.to_string())?;
    if let Some(tx) = signals.get(&upload_id) {
        tx.send(UploadSignal::Paused)
            .map_err(|e| message_error("发送暂停信号", e).to_string())?;
        Ok(())
    } else {
        warn!("[上传控制] 暂停失败，未找到任务 id={}", upload_id);
        Err(UploadError::UploadNotFound(upload_id).to_string())
    }
}

/// 向运行中的上传任务发送取消信号。
pub(crate) fn upload_cancel(upload_id: String) -> Result<(), String> {
    info!("[上传控制] 请求取消 id={}", upload_id);
    let signals = upload_signal_registry().map_err(|err| err.to_string())?;
    if let Some(tx) = signals.get(&upload_id) {
        tx.send(UploadSignal::Cancelled)
            .map_err(|e| message_error("发送取消信号", e).to_string())?;
        Ok(())
    } else {
        warn!("[上传控制] 取消失败，未找到任务 id={}", upload_id);
        Err(UploadError::UploadNotFound(upload_id).to_string())
    }
}
