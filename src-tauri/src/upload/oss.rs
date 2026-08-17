//! 真实的 OSS 上传执行器。
//!
//! 这个模块只关心“如何把一个本地文件传到 OSS”，不负责任务排队、数据库持久化或
//! 115 接口协商。它支持：
//! - 简单上传与分片上传
//! - 断点续传
//! - STS 凭证临期中止
//! - 运行中暂停/取消信号
//! - 向 Tauri 事件总线和内部 hook 双路发送进度事件

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use ali_oss_rs::ClientBuilder;
use ali_oss_rs::error::Error as OssError;
use ali_oss_rs::multipart::MultipartUploadsOperations;
use ali_oss_rs::multipart_common::{
    CompleteMultipartUploadOptions, CompleteMultipartUploadRequest, ListPartsOptions,
    ListPartsResult, ListPartsResultItem, UploadPartRequest,
};
use ali_oss_rs::object::ObjectOperations;
use ali_oss_rs::object_common::{Callback, CallbackBodyType, PutObjectOptionsBuilder};
use ali_oss_rs::reqwest::{Client as HttpClient, Proxy};
use log::{error, info, warn};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;

use super::control::{UploadSignal, upload_signal_registry};
use super::error::{UploadError, UploadResult, io_error, message_error};

const UPLOAD_PROXY_ENV: &str = "OOF_UPLOAD_PROXY";
const LIST_PARTS_PAGE_SIZE: u32 = 1000;

/// 前端上传代理设置的一次任务级快照。
///
/// `url` 即使在关闭时也会保留，用来区分“用户显式关闭”与“从未配置”；后者允许读取
/// `OOF_UPLOAD_PROXY` 作为启动级 fallback。
#[derive(Clone, Default)]
pub(crate) struct UploadProxyConfig {
    enabled: bool,
    url: String,
}

impl UploadProxyConfig {
    pub(crate) fn new(enabled: bool, url: String) -> Self {
        Self {
            enabled,
            url: url.trim().to_string(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UploadProxySource {
    Setting,
    Environment,
}

impl UploadProxySource {
    fn label(self) -> &'static str {
        match self {
            Self::Setting => "setting",
            Self::Environment => "environment",
        }
    }
}

struct EffectiveUploadProxy {
    url: String,
    source: UploadProxySource,
}

#[derive(Default)]
struct ListedParts {
    parts: Vec<ListPartsResultItem>,
    next_marker: Option<u32>,
}

impl ListedParts {
    fn push_page(&mut self, page: ListPartsResult) -> ali_oss_rs::Result<bool> {
        let next_marker = if page.is_truncated {
            let marker = page.next_part_number_marker.ok_or_else(|| {
                OssError::Other("OSS 分片列表已截断，但响应缺少下一页游标".to_string())
            })?;
            let current_marker = self.next_marker.unwrap_or(0);
            if marker <= current_marker {
                return Err(OssError::Other(format!(
                    "OSS 分片列表分页游标未前进: current={current_marker}, next={marker}"
                )));
            }
            Some(marker)
        } else {
            None
        };

        self.parts.extend(page.parts);
        self.next_marker = next_marker;
        Ok(next_marker.is_some())
    }
}

fn resolve_upload_proxy(
    config: &UploadProxyConfig,
    environment_proxy: Option<&str>,
) -> Option<EffectiveUploadProxy> {
    if config.enabled {
        return Some(EffectiveUploadProxy {
            url: config.url.clone(),
            source: UploadProxySource::Setting,
        });
    }

    // 地址非空说明用户保存过 UI 配置并显式关闭，此时不能再被环境变量重新启用。
    if !config.url.is_empty() {
        return None;
    }

    environment_proxy
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(|url| EffectiveUploadProxy {
            url: url.to_string(),
            source: UploadProxySource::Environment,
        })
}

async fn list_all_uploaded_parts(
    client: &ali_oss_rs::Client,
    bucket: &str,
    object: &str,
    oss_upload_id: &str,
) -> ali_oss_rs::Result<Vec<ListPartsResultItem>> {
    let mut listed = ListedParts::default();

    loop {
        let page = client
            .list_parts(
                bucket,
                object,
                oss_upload_id,
                Some(ListPartsOptions {
                    max_parts: Some(LIST_PARTS_PAGE_SIZE),
                    part_number_marker: listed.next_marker,
                }),
            )
            .await?;

        if !listed.push_page(page)? {
            return Ok(listed.parts);
        }
    }
}

async fn initiate_sequential_upload(
    client: &ali_oss_rs::Client,
    bucket: &str,
    object: &str,
    app: &AppHandle,
    hooks: &UploadHooks,
    upload_id: &str,
    source: &str,
) -> UploadResult<String> {
    let init_options = PutObjectOptionsBuilder::new()
        .parameter("sequential", "")
        .build();
    let init_result = client
        .initiate_multipart_uploads(bucket, object, Some(init_options))
        .await
        .map_err(|e| message_error("初始化分片上传", e))?;
    let new_id = init_result.upload_id;

    info!(
        "[上传任务][{}] 创建分片会话 oss_upload_id={} 来源={}",
        upload_id, new_id, source
    );
    emit_oss_init(
        app,
        hooks,
        OssUploadInitEvent {
            upload_id: upload_id.to_string(),
            oss_upload_id: new_id.clone(),
        },
    );

    Ok(new_id)
}

fn is_part_already_exist(error: &OssError) -> bool {
    matches!(error, OssError::ApiError(response) if response.code == "PartAlreadyExist")
}

/// 上传中的进度快照事件。
#[derive(serde::Serialize, Clone)]
pub(crate) struct UploadProgressEvent {
    pub upload_id: String,
    pub uploaded_size: u64,
    pub total_size: u64,
    pub part_number: u32,
    pub total_parts: u32,
    pub status: String,
}

/// 分片上传初始化成功后回传给上层的 OSS upload id。
#[derive(serde::Serialize, Clone)]
pub(crate) struct OssUploadInitEvent {
    pub upload_id: String,
    pub oss_upload_id: String,
}

/// 给上层留出的可选钩子。
///
/// Tauri 事件是给前端消费的，hook 则用于上传队列把进度同步写回数据库。
#[derive(Clone, Default)]
pub(crate) struct UploadHooks {
    pub on_progress: Option<Arc<dyn Fn(UploadProgressEvent) + Send + Sync>>,
    pub on_oss_init: Option<Arc<dyn Fn(OssUploadInitEvent) + Send + Sync>>,
}

/// 供上传队列内部复用的 OSS 上传入口。
///
/// 这里保留结构化错误和内部 hook，便于调度器做更精细的状态处理与数据库同步。
pub(crate) async fn upload_to_oss_internal(
    app: AppHandle,
    upload_id: String,
    file_path: String,
    bucket: String,
    object: String,
    endpoint: String,
    access_key_id: String,
    access_key_secret: String,
    security_token: String,
    callback: String,
    callback_var: String,
    oss_upload_id: Option<String>,
    token_expiration_ms: Option<u64>,
    upload_proxy: UploadProxyConfig,
    hooks: UploadHooks,
) -> UploadResult<String> {
    info!(
        "[上传任务][{}] 开始上传 path={} bucket={} object={} oss_upload_id={:?}",
        upload_id, file_path, bucket, object, oss_upload_id
    );

    // 每个上传任务都拥有一条独立的 watch 信号，用于暂停/取消控制。
    let (tx, rx) = watch::channel(UploadSignal::Running);
    {
        let mut signals = upload_signal_registry()?;
        signals.insert(upload_id.clone(), tx);
    }

    let result = upload_file_impl(
        app,
        upload_id.clone(),
        file_path,
        bucket,
        object,
        endpoint,
        access_key_id,
        access_key_secret,
        security_token,
        callback,
        callback_var,
        oss_upload_id,
        token_expiration_ms,
        upload_proxy,
        rx,
        hooks,
    )
    .await;

    {
        let mut signals = upload_signal_registry()?;
        signals.remove(&upload_id);
    }

    match &result {
        Ok(_) => info!("[上传任务][{}] 上传完成", upload_id),
        Err(e) => error!("[上传任务][{}] 上传失败: {}", upload_id, e),
    }

    result
}

/// 单个文件上传的核心实现。
///
/// 决策流程如下：
/// 1. 读取文件元数据并构造 OSS 客户端
/// 2. 根据文件大小决定简单上传或分片上传
/// 3. 如果带有 `oss_upload_id`，优先尝试断点续传
/// 4. 分片循环中持续检查控制信号与 STS 过期时间
/// 5. 上传完成后发出完成事件
async fn upload_file_impl(
    app: AppHandle,
    upload_id: String,
    file_path: String,
    bucket: String,
    object: String,
    endpoint: String,
    access_key_id: String,
    access_key_secret: String,
    security_token: String,
    callback: String,
    callback_var: String,
    oss_upload_id: Option<String>,
    token_expiration_ms: Option<u64>,
    upload_proxy: UploadProxyConfig,
    rx: watch::Receiver<UploadSignal>,
    hooks: UploadHooks,
) -> UploadResult<String> {
    let file_meta =
        std::fs::metadata(&file_path).map_err(|e| io_error("读取文件元数据", &file_path, e))?;
    let file_size = file_meta.len();
    info!(
        "[上传任务][{}] 文件大小={:.1}MB",
        upload_id,
        file_size as f64 / 1024.0 / 1024.0
    );

    // STS 提前 5 分钟判定为不可用，避免上传过程中踩到边界时间。
    let token_deadline_ms: Option<u64> =
        token_expiration_ms.map(|ms| ms.saturating_sub(5 * 60 * 1000));

    // 强制使用 HTTPS，并兼容前端传入的 endpoint 已带 scheme 的情况。
    let clean_endpoint = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let mut oss_builder = ClientBuilder::new(&access_key_id, &access_key_secret, clean_endpoint)
        .sts_token(&security_token)
        .scheme("https");

    let environment_proxy = std::env::var(UPLOAD_PROXY_ENV).ok();
    if let Some(effective_proxy) = resolve_upload_proxy(&upload_proxy, environment_proxy.as_deref())
    {
        if effective_proxy.url.is_empty() {
            return Err(message_error("解析上传代理", "代理地址不能为空"));
        }

        let proxy = Proxy::all(effective_proxy.url.as_str())
            .map_err(|e| message_error("解析上传代理", e))?;
        let http_client = HttpClient::builder()
            .proxy(proxy)
            .build()
            .map_err(|e| message_error("创建上传代理客户端", e))?;

        oss_builder = oss_builder.client(http_client);
        info!(
            "[上传任务][{}] 已启用上传代理 source={}",
            upload_id,
            effective_proxy.source.label()
        );
    }

    let client = oss_builder
        .build()
        .map_err(|e| message_error("创建 OSS 客户端", e))?;

    // OSS 最多支持 10000 个分片，这里动态放大分片尺寸，避免超出上限。
    let min_part_size: u64 = 5 * 1024 * 1024;
    let max_parts: u64 = 10000;
    let part_size: u64 = if file_size > min_part_size * max_parts {
        let needed = (file_size + max_parts - 1) / max_parts;
        let mb = 1024 * 1024;
        ((needed + mb - 1) / mb) * mb
    } else {
        min_part_size
    };
    let total_parts = file_size.div_ceil(part_size) as u32;

    // 小文件直接走简单上传，避免无意义地初始化分片会话。
    if file_size <= part_size && oss_upload_id.is_none() {
        info!(
            "[上传任务][{}] 使用简单上传 (文件 <= {}MB)",
            upload_id,
            part_size / 1024 / 1024
        );
        return simple_upload_impl(
            &app,
            &upload_id,
            &file_path,
            file_size,
            &bucket,
            &object,
            &client,
            &callback,
            &callback_var,
            &hooks,
        )
        .await;
    }

    let mut upload_results: Vec<(u32, String)> = Vec::new();
    let mut uploaded_size: u64 = 0;
    let mut completed_parts: HashSet<u32> = HashSet::new();

    info!(
        "[上传任务][{}] 使用分片上传 part_size={}MB total_parts={}",
        upload_id,
        part_size / 1024 / 1024,
        total_parts
    );

    // 优先复用旧 upload id；ListParts 单页最多返回 1000 条，必须读取全部分页。
    let mut current_oss_upload_id = if let Some(ref existing_id) = oss_upload_id {
        match list_all_uploaded_parts(&client, &bucket, &object, existing_id).await {
            Ok(parts) => {
                for part in &parts {
                    upload_results.push((part.part_number, part.etag.clone()));
                    completed_parts.insert(part.part_number);
                    uploaded_size += part.size;
                }
                info!(
                    "[上传任务][{}] 断点探测成功 oss_upload_id={} 已完成分片={} 已上传={}B",
                    upload_id,
                    existing_id,
                    completed_parts.len(),
                    uploaded_size
                );
                emit_progress(
                    &app,
                    &hooks,
                    UploadProgressEvent {
                        upload_id: upload_id.clone(),
                        uploaded_size,
                        total_size: file_size,
                        part_number: completed_parts.len() as u32,
                        total_parts,
                        status: "uploading".to_string(),
                    },
                );
                existing_id.clone()
            }
            Err(err) => {
                warn!(
                    "[上传任务][{}] 断点探测失败 oss_upload_id={}，重新初始化: {}",
                    upload_id, existing_id, err
                );
                initiate_sequential_upload(
                    &client,
                    &bucket,
                    &object,
                    &app,
                    &hooks,
                    &upload_id,
                    "断点探测失败重建",
                )
                .await?
            }
        }
    } else {
        initiate_sequential_upload(&client, &bucket, &object, &app, &hooks, &upload_id, "新建")
            .await?
    };

    let mut ranges: Vec<(u32, Range<u64>)> = Vec::new();
    let mut offset: u64 = 0;
    let mut part_number: u32 = 1;
    while offset < file_size {
        let end = std::cmp::min(offset + part_size, file_size);
        ranges.push((part_number, offset..end));
        offset = end;
        part_number += 1;
    }

    let mut reset_after_part_conflict = false;

    'upload_session: loop {
        for (part_num, range) in &ranges {
            // 已完成的分片不重复上传，这也是断点续传生效的关键。
            if completed_parts.contains(part_num) {
                continue;
            }

            // 每个分片开始前都重新检查 STS 是否安全可用。
            if let Some(deadline_ms) = token_deadline_ms {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                if now_ms >= deadline_ms {
                    warn!(
                        "[上传任务][{}] STS 凭证即将过期，终止本次上传 deadline_ms={} now_ms={}",
                        upload_id, deadline_ms, now_ms
                    );
                    return Err(UploadError::TokenExpired);
                }
            }

            {
                // 分片边界是最稳定的暂停/取消检查点：不破坏已完成分片，也不会丢掉进度。
                let signal = rx.borrow().clone();
                match signal {
                    UploadSignal::Paused => {
                        info!("[上传任务][{}] 收到控制信号: paused", upload_id);
                        return Err(UploadError::Paused);
                    }
                    UploadSignal::Cancelled => {
                        info!("[上传任务][{}] 收到控制信号: cancelled", upload_id);
                        let _ = client
                            .abort_multipart_uploads(&bucket, &object, &current_oss_upload_id)
                            .await;
                        return Err(UploadError::Cancelled);
                    }
                    UploadSignal::Running => {}
                }
            }

            let upload_data = UploadPartRequest {
                part_number: *part_num,
                upload_id: current_oss_upload_id.clone(),
            };

            let upload_result = match client
                .upload_part_from_file(&bucket, &object, &file_path, range.clone(), upload_data)
                .await
            {
                Ok(result) => result,
                Err(err) if is_part_already_exist(&err) && !reset_after_part_conflict => {
                    warn!(
                        "[上传任务][{}] 顺序分片冲突 oss_upload_id={} part={}，废弃会话并从头重传",
                        upload_id, current_oss_upload_id, part_num
                    );
                    if let Err(abort_err) = client
                        .abort_multipart_uploads(&bucket, &object, &current_oss_upload_id)
                        .await
                    {
                        warn!(
                            "[上传任务][{}] 废弃冲突会话失败 oss_upload_id={}: {}",
                            upload_id, current_oss_upload_id, abort_err
                        );
                    }
                    current_oss_upload_id = initiate_sequential_upload(
                        &client,
                        &bucket,
                        &object,
                        &app,
                        &hooks,
                        &upload_id,
                        "PartAlreadyExist重建",
                    )
                    .await?;
                    upload_results.clear();
                    completed_parts.clear();
                    uploaded_size = 0;
                    reset_after_part_conflict = true;
                    emit_progress(
                        &app,
                        &hooks,
                        UploadProgressEvent {
                            upload_id: upload_id.clone(),
                            uploaded_size,
                            total_size: file_size,
                            part_number: 0,
                            total_parts,
                            status: "uploading".to_string(),
                        },
                    );
                    continue 'upload_session;
                }
                Err(err) => {
                    return Err(UploadError::Message {
                        action: "上传分片",
                        detail: format!("分片 {} 上传失败：{}", part_num, err),
                    });
                }
            };

            upload_results.push((*part_num, upload_result.etag));
            uploaded_size += range.end - range.start;

            emit_progress(
                &app,
                &hooks,
                UploadProgressEvent {
                    upload_id: upload_id.clone(),
                    uploaded_size,
                    total_size: file_size,
                    part_number: *part_num,
                    total_parts,
                    status: "uploading".to_string(),
                },
            );
        }
        break;
    }

    upload_results.sort_by_key(|(n, _)| *n);

    let complete_request = CompleteMultipartUploadRequest {
        upload_id: current_oss_upload_id,
        parts: upload_results,
    };

    let options = build_complete_options(&callback, &callback_var);

    let _resp = client
        .complete_multipart_uploads(&bucket, &object, complete_request, options)
        .await
        .map_err(|e| message_error("完成分片上传", e))?;

    emit_progress(
        &app,
        &hooks,
        UploadProgressEvent {
            upload_id: upload_id.clone(),
            uploaded_size: file_size,
            total_size: file_size,
            part_number: total_parts,
            total_parts,
            status: "complete".to_string(),
        },
    );

    Ok("ok".to_string())
}

/// 把 115 返回的回调 JSON 转换成 ali-oss-rs 可消费的回调结构。
fn parse_115_callback(callback_str: &str, callback_var_str: &str) -> Option<Callback> {
    let cb: serde_json::Value = serde_json::from_str(callback_str).ok()?;

    let url = cb.get("callbackUrl")?.as_str()?;
    let body = cb.get("callbackBody")?.as_str()?;
    let body_type_str = cb
        .get("callbackBodyType")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let body_type = if body_type_str.contains("json") {
        Some(CallbackBodyType::Json)
    } else {
        Some(CallbackBodyType::FormUrlEncoded)
    };

    let mut custom_variables = HashMap::new();
    if !callback_var_str.is_empty() {
        if let Ok(vars) =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(callback_var_str)
        {
            for (k, v) in vars {
                if let Some(s) = v.as_str() {
                    custom_variables.insert(k, s.to_string());
                }
            }
        }
    }

    Some(Callback {
        url: url.to_string(),
        host: None,
        body: body.to_string(),
        sni: None,
        body_type,
        custom_variables,
    })
}

fn build_complete_options(
    callback: &str,
    callback_var: &str,
) -> Option<CompleteMultipartUploadOptions> {
    if callback.is_empty() {
        return None;
    }
    let cb = parse_115_callback(callback, callback_var)?;
    Some(CompleteMultipartUploadOptions { callback: Some(cb) })
}

fn build_put_options(
    callback: &str,
    callback_var: &str,
) -> Option<ali_oss_rs::object_common::PutObjectOptions> {
    if callback.is_empty() {
        return None;
    }
    let cb = parse_115_callback(callback, callback_var)?;
    Some(PutObjectOptionsBuilder::new().callback(cb).build())
}

async fn simple_upload_impl(
    app: &AppHandle,
    upload_id: &str,
    file_path: &str,
    file_size: u64,
    bucket: &str,
    object: &str,
    client: &ali_oss_rs::Client,
    callback: &str,
    callback_var: &str,
    hooks: &UploadHooks,
) -> UploadResult<String> {
    let options = build_put_options(callback, callback_var);

    client
        .put_object_from_file(bucket, object, file_path, options)
        .await
        .map_err(|e| message_error("上传文件", e))?;

    emit_progress(
        app,
        hooks,
        UploadProgressEvent {
            upload_id: upload_id.to_string(),
            uploaded_size: file_size,
            total_size: file_size,
            part_number: 1,
            total_parts: 1,
            status: "complete".to_string(),
        },
    );

    Ok("ok".to_string())
}

fn emit_progress(app: &AppHandle, hooks: &UploadHooks, event: UploadProgressEvent) {
    let _ = app.emit("upload-progress", &event);
    if let Some(callback) = &hooks.on_progress {
        callback(event);
    }
}

fn emit_oss_init(app: &AppHandle, hooks: &UploadHooks, event: OssUploadInitEvent) {
    let _ = app.emit("upload-oss-init", &event);
    if let Some(callback) = &hooks.on_oss_init {
        callback(event);
    }
}

#[cfg(test)]
mod tests {
    use ali_oss_rs::error::ErrorResponse;

    use super::{
        ClientBuilder, HttpClient, ListPartsResult, ListPartsResultItem, ListedParts, OssError,
        Proxy, UploadProxyConfig, UploadProxySource, is_part_already_exist, resolve_upload_proxy,
    };

    fn listed_part(part_number: u32) -> ListPartsResultItem {
        ListPartsResultItem {
            part_number,
            etag: format!("etag-{part_number}"),
            size: 5 * 1024 * 1024,
            last_modified: String::new(),
        }
    }

    #[test]
    fn collects_more_than_one_thousand_listed_parts() {
        let mut listed = ListedParts::default();
        let first_page = ListPartsResult {
            next_part_number_marker: Some(1000),
            is_truncated: true,
            parts: (1..=1000).map(listed_part).collect(),
            ..ListPartsResult::default()
        };

        assert!(listed.push_page(first_page).unwrap());
        assert_eq!(listed.next_marker, Some(1000));

        let last_page = ListPartsResult {
            is_truncated: false,
            parts: (1001..=1005).map(listed_part).collect(),
            ..ListPartsResult::default()
        };

        assert!(!listed.push_page(last_page).unwrap());
        assert_eq!(listed.next_marker, None);
        assert_eq!(listed.parts.len(), 1005);
        assert_eq!(listed.parts.last().unwrap().part_number, 1005);
    }

    #[test]
    fn rejects_truncated_page_without_next_marker() {
        let mut listed = ListedParts::default();
        let page = ListPartsResult {
            is_truncated: true,
            parts: vec![listed_part(1)],
            ..ListPartsResult::default()
        };

        assert!(listed.push_page(page).is_err());
    }

    #[test]
    fn rejects_list_parts_marker_that_does_not_advance() {
        let mut listed = ListedParts {
            next_marker: Some(1000),
            ..ListedParts::default()
        };
        let page = ListPartsResult {
            next_part_number_marker: Some(1000),
            is_truncated: true,
            parts: vec![listed_part(1001)],
            ..ListPartsResult::default()
        };

        assert!(listed.push_page(page).is_err());
    }

    #[test]
    fn identifies_part_already_exist_api_error() {
        let conflict = OssError::ApiError(Box::new(ErrorResponse {
            code: "PartAlreadyExist".to_string(),
            ..ErrorResponse::default()
        }));
        let other = OssError::ApiError(Box::new(ErrorResponse {
            code: "NoSuchUpload".to_string(),
            ..ErrorResponse::default()
        }));

        assert!(is_part_already_exist(&conflict));
        assert!(!is_part_already_exist(&other));
    }

    #[test]
    fn enabled_setting_takes_priority_over_environment() {
        let config = UploadProxyConfig::new(true, " http://127.0.0.1:7897 ".to_string());
        let proxy = resolve_upload_proxy(&config, Some("http://127.0.0.1:7898")).unwrap();

        assert_eq!(proxy.url, "http://127.0.0.1:7897");
        assert!(proxy.source == UploadProxySource::Setting);
    }

    #[test]
    fn disabled_saved_setting_suppresses_environment_fallback() {
        let config = UploadProxyConfig::new(false, "http://127.0.0.1:7897".to_string());

        assert!(resolve_upload_proxy(&config, Some("http://127.0.0.1:7898")).is_none());
    }

    #[test]
    fn enabled_blank_setting_does_not_fall_back_to_environment() {
        let config = UploadProxyConfig::new(true, "   ".to_string());
        let proxy = resolve_upload_proxy(&config, Some("http://127.0.0.1:7898")).unwrap();

        assert!(proxy.url.is_empty());
        assert!(proxy.source == UploadProxySource::Setting);
    }

    #[test]
    fn empty_unconfigured_setting_uses_environment_fallback() {
        let config = UploadProxyConfig::default();
        let proxy = resolve_upload_proxy(&config, Some(" http://127.0.0.1:7897 ")).unwrap();

        assert_eq!(proxy.url, "http://127.0.0.1:7897");
        assert!(proxy.source == UploadProxySource::Environment);
    }

    #[test]
    fn blank_environment_proxy_is_ignored() {
        let config = UploadProxyConfig::default();

        assert!(resolve_upload_proxy(&config, Some("   ")).is_none());
    }

    #[test]
    fn ali_oss_builder_accepts_reexported_proxy_client() {
        let proxy = Proxy::all("http://127.0.0.1:7897").unwrap();
        let http_client = HttpClient::builder().proxy(proxy).build().unwrap();

        let result = ClientBuilder::new(
            "access_key_id",
            "access_key_secret",
            "oss-cn-hangzhou.aliyuncs.com",
        )
        .client(http_client)
        .build();

        assert!(result.is_ok());
    }
}
