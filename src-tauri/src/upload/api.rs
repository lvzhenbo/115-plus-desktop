//! 上传 API 与协议协调层。
//!
//! Rust 不直接调用 115 HTTP 接口，而是把请求描述发给前端执行，再通过 request id
//! 等待对应结果回填回来。这一层负责桥接、协议载荷定义，以及上传前的计划协商。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use log::{error, info, warn};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use tauri::{App, AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use super::local::compute_partial_sha1_internal;
use super::queue::UploadQueueError;
use super::store::{DbHandle, TaskUpdate, UploadStoreError, UploadTask};

const ERR_API_RESOLVER_POISONED: &str = "上传接口协调器状态异常：内部锁已损坏";

/// 发往前端的上传 API 请求事件。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadApiRequestEvent {
    request_id: String,
    #[serde(flatten)]
    request: UploadApiRequest,
}

/// Rust 调度器委托前端执行的 115 API 请求类型。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(super) enum UploadApiRequest {
    Init {
        task_id: String,
        file_name: String,
        file_size: i64,
        target: String,
        fileid: String,
        preid: Option<String>,
        pick_code: Option<String>,
        sign_key: Option<String>,
        sign_val: Option<String>,
    },
    Resume {
        task_id: String,
        file_size: i64,
        target: String,
        fileid: String,
        pick_code: String,
    },
    Token {
        task_id: String,
    },
    CreateFolder {
        task_id: String,
        file_name: String,
        parent_cid: String,
    },
}

impl UploadApiRequest {
    fn kind(&self) -> &'static str {
        match self {
            Self::Init { .. } => "init",
            Self::Resume { .. } => "resume",
            Self::Token { .. } => "token",
            Self::CreateFolder { .. } => "createFolder",
        }
    }

    fn task_id(&self) -> &str {
        match self {
            Self::Init { task_id, .. }
            | Self::Resume { task_id, .. }
            | Self::Token { task_id }
            | Self::CreateFolder { task_id, .. } => task_id,
        }
    }
}

/// 初始化/续传接口返回里的 OSS 回调配置。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct UploadCallback {
    pub(super) callback: String,
    pub(super) callback_var: String,
}

fn deserialize_optional_upload_callback<'de, D>(
    deserializer: D,
) -> Result<Option<UploadCallback>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) if items.is_empty() => Ok(None),
        Some(Value::Object(fields)) if fields.is_empty() => Ok(None),
        Some(other) => serde_json::from_value(other)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

/// 初始化上传接口中调度器真正关心的字段。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct UploadInitData {
    pub(super) pick_code: String,
    pub(super) status: u8,
    pub(super) sign_key: Option<String>,
    pub(super) sign_check: Option<String>,
    pub(super) file_id: Option<String>,
    pub(super) target: Option<String>,
    pub(super) bucket: Option<String>,
    pub(super) object: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_upload_callback")]
    pub(super) callback: Option<UploadCallback>,
}

/// 续传接口中调度器真正关心的字段。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct UploadResumeData {
    pub(super) pick_code: String,
    pub(super) target: String,
    pub(super) version: Option<String>,
    pub(super) bucket: String,
    pub(super) object: String,
    #[serde(default, deserialize_with = "deserialize_optional_upload_callback")]
    pub(super) callback: Option<UploadCallback>,
}

/// OSS 临时凭证的最小子集。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct UploadTokenData {
    pub(super) endpoint: String,
    #[serde(rename = "AccessKeySecret")]
    pub(super) access_key_secret: String,
    #[serde(rename = "SecurityToken")]
    pub(super) security_token: String,
    #[serde(rename = "Expiration")]
    pub(super) expiration: String,
    #[serde(rename = "AccessKeyId")]
    pub(super) access_key_id: String,
}

/// 远端创建目录接口的最小返回体。
#[derive(Debug, Clone, Deserialize)]
pub(super) struct CreateFolderResponse {
    pub(super) file_id: String,
}

/// 单文件在进入真实上传前协商得到的执行计划。
#[derive(Debug)]
pub(super) struct PreparedUploadPlan {
    pub(super) file_id: Option<String>,
    pub(super) bucket: Option<String>,
    pub(super) object: Option<String>,
    pub(super) callback: Option<UploadCallback>,
    pub(super) oss_upload_id: Option<String>,
}

/// 前后端之间的上传 API 协调器。
pub struct UploadApiResolver {
    pending: Mutex<HashMap<String, PendingUploadRequest>>,
}

struct PendingUploadRequest {
    kind: &'static str,
    task_id: String,
    sender: oneshot::Sender<Result<Value, String>>,
}

impl UploadApiResolver {
    fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    fn pending_lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, PendingUploadRequest>>, UploadQueueError>
    {
        self.pending
            .lock()
            .map_err(|_| UploadQueueError::Internal(ERR_API_RESOLVER_POISONED.into()))
    }

    pub(super) async fn request<T>(
        &self,
        app: &AppHandle,
        request: UploadApiRequest,
    ) -> Result<T, UploadQueueError>
    where
        T: DeserializeOwned,
    {
        let request_id = Uuid::new_v4().to_string();
        let request_kind = request.kind();
        let task_id = request.task_id().to_string();
        let (tx, rx) = oneshot::channel();

        self.pending_lock()?.insert(
            request_id.clone(),
            PendingUploadRequest {
                kind: request_kind,
                task_id: task_id.clone(),
                sender: tx,
            },
        );

        info!(
            "[上传API][{}] 发起{}请求 request_id={}",
            task_id, request_kind, request_id
        );

        if let Err(err) = app.emit(
            "upload:api-needed",
            &UploadApiRequestEvent {
                request_id: request_id.clone(),
                request,
            },
        ) {
            self.pending_lock()?.remove(&request_id);
            error!(
                "[上传API][{}] {}请求分发失败 request_id={}: {}",
                task_id, request_kind, request_id, err
            );
            return Err(UploadQueueError::Internal(format!(
                "上传接口请求分发失败 kind={} request_id={}: {}",
                request_kind, request_id, err
            )));
        }

        match timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(Ok(payload))) => match serde_json::from_value(payload.clone()) {
                Ok(parsed) => Ok(parsed),
                Err(err) => {
                    error!(
                        "[上传API][{}] {}响应解析失败 request_id={}: {} payload={}",
                        task_id, request_kind, request_id, err, payload
                    );
                    Err(err.into())
                }
            },
            Ok(Ok(Err(message))) => Err(UploadQueueError::Internal(message)),
            Ok(Err(_)) => {
                error!(
                    "[上传API][{}] {}响应通道提前关闭 request_id={}",
                    task_id, request_kind, request_id
                );
                Err(UploadQueueError::Internal(format!(
                    "上传接口响应通道提前关闭 kind={} request_id={}",
                    request_kind, request_id
                )))
            }
            _ => {
                let timeout_meta = self
                    .pending_lock()?
                    .remove(&request_id)
                    .map(|pending| (pending.kind, pending.task_id))
                    .unwrap_or((request_kind, task_id.clone()));
                error!(
                    "[上传API][{}] {}请求超时 request_id={}",
                    timeout_meta.1, timeout_meta.0, request_id
                );
                Err(UploadQueueError::Internal(format!(
                    "上传接口请求超时 kind={} request_id={}",
                    timeout_meta.0, request_id
                )))
            }
        }
    }

    fn provide_response(&self, request_id: &str, payload: Value) -> Result<(), UploadQueueError> {
        let sender = self.pending_lock()?.remove(request_id);
        match sender {
            Some(pending) => {
                let _ = pending.sender.send(Ok(payload));
                Ok(())
            }
            None => Err(UploadQueueError::NotFound(format!(
                "未找到 request_id={} 对应的上传接口请求",
                request_id
            ))),
        }
    }

    fn provide_error(
        &self,
        request_id: &str,
        error_message: String,
    ) -> Result<(), UploadQueueError> {
        let sender = self.pending_lock()?.remove(request_id);
        match sender {
            Some(pending) => {
                error!(
                    "[上传API][{}] {}请求失败 request_id={}: {}",
                    pending.task_id, pending.kind, request_id, error_message
                );
                let _ = pending.sender.send(Err(error_message));
                Ok(())
            }
            None => Err(UploadQueueError::NotFound(format!(
                "未找到 request_id={} 对应的上传接口请求",
                request_id
            ))),
        }
    }
}

/// 决定当前任务应采用哪种上传计划。
///
/// 优先级为：续传计划 -> 初始化计划（其中可能再进入二次认证）。
pub(super) async fn prepare_upload_plan(
    task_id: &str,
    task: &UploadTask,
    sha1: &str,
    pre_sha1: &str,
    app: &AppHandle,
    db: &DbHandle,
    api_resolver: &Arc<UploadApiResolver>,
) -> Result<PreparedUploadPlan, String> {
    let target = format!("U_1_{}", task.target_cid);

    if let Some(pick_code) = non_empty_string(task.pick_code.clone()) {
        match request_resume_plan(
            task_id,
            task,
            sha1,
            &pick_code,
            &target,
            app,
            db,
            api_resolver,
        )
        .await
        {
            Ok(plan) => return Ok(plan),
            Err(err) => {
                warn!("[上传API][{}] 续传失败，回退完整上传: {}", task_id, err);
            }
        }
    }

    request_init_plan(
        task_id,
        task,
        sha1,
        pre_sha1,
        &target,
        app,
        db,
        api_resolver,
    )
    .await
}

/// 委托前端创建远端目录，并返回新目录的 file_id。
pub(super) async fn request_create_folder(
    app: &AppHandle,
    api_resolver: &Arc<UploadApiResolver>,
    task_id: &str,
    file_name: String,
    parent_cid: String,
) -> Result<String, UploadQueueError> {
    let data: CreateFolderResponse = api_resolver
        .request(
            app,
            UploadApiRequest::CreateFolder {
                task_id: task_id.to_string(),
                file_name,
                parent_cid,
            },
        )
        .await?;
    Ok(data.file_id)
}

async fn request_resume_plan(
    task_id: &str,
    task: &UploadTask,
    sha1: &str,
    pick_code: &str,
    target: &str,
    app: &AppHandle,
    db: &DbHandle,
    api_resolver: &Arc<UploadApiResolver>,
) -> Result<PreparedUploadPlan, String> {
    let mut reset_oss_upload_id = false;
    let raw: Value = api_resolver
        .request(
            app,
            UploadApiRequest::Resume {
                task_id: task_id.to_string(),
                file_size: task.file_size,
                target: target.to_string(),
                fileid: sha1.to_string(),
                pick_code: pick_code.to_string(),
            },
        )
        .await
        .map_err(|err| err.to_string())?;
    let raw_payload = raw.to_string();
    let data: UploadResumeData = serde_json::from_value(raw).map_err(|err| {
        error!(
            "[上传API][{}] 续传响应解析失败: {} payload={}",
            task_id, err, raw_payload
        );
        err.to_string()
    })?;

    let _ = data.target.as_str();
    let _ = data.version.as_deref();

    let mut valid_oss_upload_id = task.oss_upload_id.clone();
    if task.oss_upload_id.is_some()
        && (task.oss_bucket.as_deref() != Some(data.bucket.as_str())
            || task.oss_object.as_deref() != Some(data.object.as_str()))
    {
        valid_oss_upload_id = None;
        reset_oss_upload_id = true;
        let _ = safe_update_task(
            db,
            task_id.to_string(),
            TaskUpdate {
                oss_upload_id: Some(None),
                ..TaskUpdate::default()
            },
        )
        .await;
    }

    let _ = safe_update_task(
        db,
        task_id.to_string(),
        TaskUpdate {
            pick_code: Some(non_empty_string(Some(data.pick_code.clone()))),
            ..TaskUpdate::default()
        },
    )
    .await;

    let callback = data.callback.ok_or_else(|| {
        error!(
            "[上传API][{}] 续传响应缺少 callback payload={}",
            task_id, raw_payload
        );
        "续传接口响应缺少 callback，无法继续上传".to_string()
    })?;

    info!(
        "[上传API][{}] 续传计划已就绪 bucket={} object={} 复用oss_upload_id={} 重置oss_upload_id={}",
        task_id,
        data.bucket,
        data.object,
        valid_oss_upload_id.is_some(),
        reset_oss_upload_id
    );

    Ok(PreparedUploadPlan {
        file_id: None,
        bucket: Some(data.bucket),
        object: Some(data.object),
        callback: Some(callback),
        oss_upload_id: valid_oss_upload_id,
    })
}

async fn request_init_plan(
    task_id: &str,
    task: &UploadTask,
    sha1: &str,
    pre_sha1: &str,
    target: &str,
    app: &AppHandle,
    db: &DbHandle,
    api_resolver: &Arc<UploadApiResolver>,
) -> Result<PreparedUploadPlan, String> {
    // 与旧版前端逻辑保持一致：完整 init 首次请求不携带旧 pick_code，
    // 只有服务端明确进入二次认证后，才回带本轮返回的新 pick_code。
    let mut pick_code: Option<String> = None;
    let mut sign_key: Option<String> = None;
    let mut sign_val: Option<String> = None;

    loop {
        let raw: Value = api_resolver
            .request(
                app,
                UploadApiRequest::Init {
                    task_id: task_id.to_string(),
                    file_name: task.file_name.clone(),
                    file_size: task.file_size,
                    target: target.to_string(),
                    fileid: sha1.to_string(),
                    preid: non_empty_string(Some(pre_sha1.to_string())),
                    pick_code: pick_code.clone(),
                    sign_key: sign_key.clone(),
                    sign_val: sign_val.clone(),
                },
            )
            .await
            .map_err(|err| err.to_string())?;
        let raw_payload = raw.to_string();
        let data: UploadInitData = serde_json::from_value(raw).map_err(|err| {
            error!(
                "[上传API][{}] 初始化响应解析失败: {} payload={}",
                task_id, err, raw_payload
            );
            err.to_string()
        })?;

        let _ = data.target.as_deref();

        let _ = safe_update_task(
            db,
            task_id.to_string(),
            TaskUpdate {
                pick_code: Some(non_empty_string(Some(data.pick_code.clone()))),
                ..TaskUpdate::default()
            },
        )
        .await;

        if let (Some(next_sign_key), Some(sign_check)) = (
            non_empty_string(data.sign_key.clone()),
            non_empty_string(data.sign_check.clone()),
        ) {
            info!(
                "[上传API][{}] 收到二次认证 sign_check={}",
                task_id, sign_check
            );
            let (start, end) = parse_sign_check(&sign_check)?;
            let partial = compute_partial_sha1_internal(task.file_path.clone(), start, end)
                .await
                .map_err(|err| err.to_string())?;
            pick_code = non_empty_string(Some(data.pick_code));
            sign_key = Some(next_sign_key);
            sign_val = Some(partial);
            continue;
        }

        if data.status == 2 {
            info!(
                "[上传API][{}] 秒传成功 file_id={}",
                task_id,
                data.file_id.as_deref().unwrap_or("")
            );
            return Ok(PreparedUploadPlan {
                file_id: data.file_id,
                bucket: None,
                object: None,
                callback: None,
                oss_upload_id: task.oss_upload_id.clone(),
            });
        }

        if data.status != 1 {
            error!(
                "[上传API][{}] 初始化响应状态异常 status={} payload={}",
                task_id, data.status, raw_payload
            );
            return Err(format!("上传初始化返回未支持状态: {}", data.status));
        }

        let bucket = data.bucket.ok_or_else(|| {
            error!(
                "[上传API][{}] 初始化响应缺少 bucket payload={}",
                task_id, raw_payload
            );
            "上传初始化响应缺少 bucket，无法继续上传".to_string()
        })?;
        let object = data.object.ok_or_else(|| {
            error!(
                "[上传API][{}] 初始化响应缺少 object payload={}",
                task_id, raw_payload
            );
            "上传初始化响应缺少 object，无法继续上传".to_string()
        })?;
        let callback = data.callback.ok_or_else(|| {
            error!(
                "[上传API][{}] 初始化响应缺少 callback payload={}",
                task_id, raw_payload
            );
            "上传初始化响应缺少 callback，无法继续上传".to_string()
        })?;

        info!(
            "[上传API][{}] 初始化计划已就绪 bucket={} object={} 已有oss_upload_id={}",
            task_id,
            bucket,
            object,
            task.oss_upload_id.is_some()
        );

        return Ok(PreparedUploadPlan {
            file_id: data.file_id,
            bucket: Some(bucket),
            object: Some(object),
            callback: Some(callback),
            oss_upload_id: task.oss_upload_id.clone(),
        });
    }
}

fn non_empty_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_sign_check(raw: &str) -> Result<(u64, u64), String> {
    let (start, end) = raw
        .split_once('-')
        .ok_or_else(|| format!("无效的 sign_check: {}", raw))?;
    let start = start
        .parse::<u64>()
        .map_err(|_| format!("无效的 sign_check start: {}", raw))?;
    let end = end
        .parse::<u64>()
        .map_err(|_| format!("无效的 sign_check end: {}", raw))?;

    if end < start {
        return Err(format!("无效的 sign_check 区间: {}", raw));
    }

    Ok((start, end))
}

async fn safe_update_task(
    db: &DbHandle,
    id: String,
    updates: TaskUpdate,
) -> Result<(), UploadQueueError> {
    match db.update_task(id, updates).await {
        Ok(()) => Ok(()),
        Err(UploadStoreError::NotFound(_)) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// 初始化上传 API 协调器，并注册为 Tauri 全局状态。
pub fn init(app: &App) {
    app.manage(Arc::new(UploadApiResolver::new()));
}

/// 前端把上传接口的成功结果回填给调度器。
#[tauri::command]
pub async fn upload_provide_api_response(
    request_id: String,
    payload: Value,
    resolver: tauri::State<'_, Arc<UploadApiResolver>>,
) -> Result<(), UploadQueueError> {
    resolver.provide_response(&request_id, payload)
}

/// 前端把上传接口的失败结果回填给调度器。
#[tauri::command]
pub async fn upload_provide_api_error(
    request_id: String,
    error_message: String,
    resolver: tauri::State<'_, Arc<UploadApiResolver>>,
) -> Result<(), UploadQueueError> {
    resolver.provide_error(&request_id, error_message)
}
