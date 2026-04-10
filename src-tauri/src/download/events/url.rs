use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use super::super::store::DmError;

const ERR_URL_RESOLVER_POISONED: &str = "下载地址协调器状态异常：内部锁已损坏";

/// download:url-needed 事件载荷。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlNeededPayload {
    pub request_id: String,
    pub task_id: String,
    pub pick_code: String,
}

/// 下载地址请求协调器。
///
/// 负责将 download:url-needed 事件和 download_provide_url 命令按 request_id 对应起来。
pub struct UrlResolver {
    pending: Mutex<HashMap<String, oneshot::Sender<String>>>,
}

impl UrlResolver {
    fn pending_lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, oneshot::Sender<String>>>, DmError> {
        self.pending
            .lock()
            .map_err(|_| DmError::Internal(ERR_URL_RESOLVER_POISONED.into()))
    }
}

impl UrlResolver {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// 由下载引擎调用，请求前端提供新的下载地址。
    ///
    /// 单次等待 30 秒，最多重试 3 次。
    pub async fn request_url(
        &self,
        app: &AppHandle,
        task_id: &str,
        pick_code: &str,
    ) -> Result<String, DmError> {
        for attempt in 0..3u8 {
            let request_id = Uuid::new_v4().to_string();
            let (tx, rx) = oneshot::channel();

            // 先登记等待中的 request_id，确保回调到达时能被正确匹配。
            {
                self.pending_lock()?.insert(request_id.clone(), tx);
            }

            // 通知前端刷新该任务的下载地址。
            let _ = app.emit(
                "download:url-needed",
                &UrlNeededPayload {
                    request_id: request_id.clone(),
                    task_id: task_id.to_string(),
                    pick_code: pick_code.to_string(),
                },
            );

            // 等待前端回传新地址，超时后清理挂起请求并重试。
            match timeout(Duration::from_secs(30), rx).await {
                Ok(Ok(url)) => return Ok(url),
                _ => {
                    self.pending_lock()?.remove(&request_id);
                    log::warn!(
                        "任务 {} 请求新下载地址超时，第 {}/3 次重试",
                        task_id,
                        attempt + 1
                    );
                }
            }
        }

        Err(DmError::Internal(format!(
            "任务 {} 获取下载地址失败：重试 3 次后仍未收到前端回传",
            task_id
        )))
    }

    /// 由 download_provide_url 命令调用，接收前端回传的新下载地址。
    pub fn provide_url(&self, request_id: &str, url: String) -> Result<(), DmError> {
        let sender = self.pending_lock()?.remove(request_id);
        match sender {
            Some(tx) => {
                let _ = tx.send(url);
                Ok(())
            }
            None => Err(DmError::NotFound(format!(
                "未找到 request_id={} 对应的待处理下载地址请求",
                request_id
            ))),
        }
    }
}

/// Tauri 命令：前端收到 download:url-needed 后通过该命令回传新的下载地址。
#[tauri::command]
pub async fn download_provide_url(
    request_id: String,
    url: String,
    resolver: tauri::State<'_, Arc<UrlResolver>>,
) -> Result<(), DmError> {
    resolver.provide_url(&request_id, url)
}
