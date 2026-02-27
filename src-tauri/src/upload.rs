use ali_oss_rs::ClientBuilder;
use ali_oss_rs::multipart::MultipartUploadsOperations;
use ali_oss_rs::multipart_common::{
    CompleteMultipartUploadOptions, CompleteMultipartUploadRequest, UploadPartRequest,
};
use ali_oss_rs::object::ObjectOperations;
use ali_oss_rs::object_common::{Callback, CallbackBodyType, PutObjectOptionsBuilder};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;

// 全局上传任务取消/暂停信号管理
lazy_static::lazy_static! {
    static ref UPLOAD_SIGNALS: Arc<Mutex<HashMap<String, watch::Sender<UploadSignal>>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

#[derive(Clone, Debug, PartialEq)]
pub enum UploadSignal {
    Running,
    Paused,
    Cancelled,
}

#[derive(serde::Serialize, Clone)]
pub struct FileHashResult {
    pub sha1: String,
    pub pre_sha1: String,
}

#[derive(serde::Serialize, Clone)]
pub struct UploadProgressEvent {
    pub upload_id: String,
    pub uploaded_size: u64,
    pub total_size: u64,
    pub part_number: u32,
    pub total_parts: u32,
    pub status: String,
}

/// OSS 分片上传初始化事件（通知前端保存 oss_upload_id 用于断点续传）
#[derive(serde::Serialize, Clone)]
pub struct OssUploadInitEvent {
    pub upload_id: String,
    pub oss_upload_id: String,
}

/// 计算文件的完整 SHA1 和前 128KB SHA1
#[tauri::command]
pub async fn compute_file_hash(file_path: String) -> Result<FileHashResult, String> {
    tokio::task::spawn_blocking(move || {
        let mut file =
            std::fs::File::open(&file_path).map_err(|e| format!("打开文件失败: {}", e))?;

        let metadata = file
            .metadata()
            .map_err(|e| format!("获取文件元数据失败: {}", e))?;
        let file_size = metadata.len();

        // 计算前 128KB SHA1
        let pre_size = std::cmp::min(file_size, 128 * 1024);
        let mut pre_buf = vec![0u8; pre_size as usize];
        file.read_exact(&mut pre_buf)
            .map_err(|e| format!("读取文件失败: {}", e))?;

        let mut pre_hasher = Sha1::new();
        pre_hasher.update(&pre_buf);
        let pre_sha1 = format!("{:X}", pre_hasher.finalize());

        // 计算完整 SHA1
        file.seek(SeekFrom::Start(0))
            .map_err(|e| format!("文件 seek 失败: {}", e))?;

        let mut hasher = Sha1::new();
        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer
        loop {
            let n = file
                .read(&mut buffer)
                .map_err(|e| format!("读取文件失败: {}", e))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        let sha1 = format!("{:X}", hasher.finalize());

        Ok(FileHashResult { sha1, pre_sha1 })
    })
    .await
    .map_err(|e| format!("计算哈希失败: {}", e))?
}

/// 计算文件指定区间的 SHA1（用于二次认证）
#[tauri::command]
pub async fn compute_partial_sha1(
    file_path: String,
    start: u64,
    end: u64,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let mut file =
            std::fs::File::open(&file_path).map_err(|e| format!("打开文件失败: {}", e))?;

        file.seek(SeekFrom::Start(start))
            .map_err(|e| format!("文件 seek 失败: {}", e))?;

        let len = (end - start + 1) as usize;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf)
            .map_err(|e| format!("读取文件失败: {}", e))?;

        let mut hasher = Sha1::new();
        hasher.update(&buf);
        Ok(format!("{:X}", hasher.finalize()))
    })
    .await
    .map_err(|e| format!("计算部分哈希失败: {}", e))?
}

/// 上传文件到 OSS（分片上传，支持暂停/取消/断点续传，发送进度事件）
#[tauri::command]
pub async fn upload_to_oss(
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
) -> Result<String, String> {
    let (tx, rx) = watch::channel(UploadSignal::Running);
    {
        let mut signals = UPLOAD_SIGNALS.lock().unwrap();
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
        rx,
    )
    .await;

    // 清理信号
    {
        let mut signals = UPLOAD_SIGNALS.lock().unwrap();
        signals.remove(&upload_id);
    }

    result
}

/// 暂停上传任务
#[tauri::command]
pub fn pause_upload(upload_id: String) -> Result<(), String> {
    let signals = UPLOAD_SIGNALS.lock().unwrap();
    if let Some(tx) = signals.get(&upload_id) {
        tx.send(UploadSignal::Paused)
            .map_err(|e| format!("发送暂停信号失败: {}", e))?;
        Ok(())
    } else {
        Err("未找到上传任务".to_string())
    }
}

/// 取消上传任务
#[tauri::command]
pub fn cancel_upload(upload_id: String) -> Result<(), String> {
    let signals = UPLOAD_SIGNALS.lock().unwrap();
    if let Some(tx) = signals.get(&upload_id) {
        tx.send(UploadSignal::Cancelled)
            .map_err(|e| format!("发送取消信号失败: {}", e))?;
        Ok(())
    } else {
        Err("未找到上传任务".to_string())
    }
}

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
    mut rx: watch::Receiver<UploadSignal>,
) -> Result<String, String> {
    let file_meta =
        std::fs::metadata(&file_path).map_err(|e| format!("获取文件信息失败: {}", e))?;
    let file_size = file_meta.len();

    // STS 凭证过期时间（毫秒时间戳），提前 5 分钟视为过期以留出安全余量
    let token_deadline_ms: Option<u64> =
        token_expiration_ms.map(|ms| ms.saturating_sub(5 * 60 * 1000));

    // 构建 OSS 客户端（强制 HTTPS，清理 endpoint 中可能携带的 scheme）
    let clean_endpoint = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let client = ClientBuilder::new(&access_key_id, &access_key_secret, clean_endpoint)
        .sts_token(&security_token)
        .scheme("https")
        .build()
        .map_err(|e| format!("创建 OSS 客户端失败: {}", e))?;

    // 动态计算分片大小：
    // - 基础分片 5MB，OSS 最多 10000 分片
    // - 如果文件过大则增大分片以控制分片数 ≤ 10000
    let min_part_size: u64 = 5 * 1024 * 1024; // 5MB
    let max_parts: u64 = 10000;
    let part_size: u64 = if file_size > min_part_size * max_parts {
        // 确保分片数不超过上限，向上取整到 MB
        let needed = (file_size + max_parts - 1) / max_parts;
        let mb = 1024 * 1024;
        ((needed + mb - 1) / mb) * mb
    } else {
        min_part_size
    };
    let total_parts = ((file_size + part_size - 1) / part_size) as u32;

    // 如果文件小于分片大小且没有需要续传的 oss_upload_id，使用简单上传
    if file_size <= part_size && oss_upload_id.is_none() {
        return simple_upload(
            &app,
            &upload_id,
            &file_path,
            file_size,
            &bucket,
            &object,
            &client,
            &callback,
            &callback_var,
        )
        .await;
    }

    // 断点续传：复用已有的 oss_upload_id，或者初始化新的分片上传
    let mut upload_results: Vec<(u32, String)> = Vec::new();
    let mut uploaded_size: u64 = 0;
    let mut completed_parts: std::collections::HashSet<u32> = std::collections::HashSet::new();

    let current_oss_upload_id = if let Some(ref existing_id) = oss_upload_id {
        // 查询已上传的分片
        match client.list_parts(&bucket, &object, existing_id, None).await {
            Ok(list_result) => {
                for part in &list_result.parts {
                    upload_results.push((part.part_number, part.etag.clone()));
                    completed_parts.insert(part.part_number);
                    uploaded_size += part.size;
                }
                // 通知前端当前进度
                let _ = app.emit(
                    "upload-progress",
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
            Err(_) => {
                // list_parts 失败（可能 oss_upload_id 已过期/不存在），重新初始化
                let init_options = PutObjectOptionsBuilder::new()
                    .parameter("sequential", "")
                    .build();
                let init_result = client
                    .initiate_multipart_uploads(&bucket, &object, Some(init_options))
                    .await
                    .map_err(|e| format!("初始化分片上传失败: {}", e))?;
                let new_id = init_result.upload_id.clone();
                // 通知前端新的 oss_upload_id
                let _ = app.emit(
                    "upload-oss-init",
                    OssUploadInitEvent {
                        upload_id: upload_id.clone(),
                        oss_upload_id: new_id.clone(),
                    },
                );
                new_id
            }
        }
    } else {
        // 全新上传：初始化分片上传
        let init_options = PutObjectOptionsBuilder::new()
            .parameter("sequential", "")
            .build();
        let init_result = client
            .initiate_multipart_uploads(&bucket, &object, Some(init_options))
            .await
            .map_err(|e| format!("初始化分片上传失败: {}", e))?;
        let new_id = init_result.upload_id.clone();
        // 通知前端保存 oss_upload_id 用于后续断点续传
        let _ = app.emit(
            "upload-oss-init",
            OssUploadInitEvent {
                upload_id: upload_id.clone(),
                oss_upload_id: new_id.clone(),
            },
        );
        new_id
    };

    // 计算分片范围
    let mut ranges: Vec<(u32, Range<u64>)> = Vec::new();
    let mut offset: u64 = 0;
    let mut part_number: u32 = 1;
    while offset < file_size {
        let end = std::cmp::min(offset + part_size, file_size);
        ranges.push((part_number, offset..end));
        offset = end;
        part_number += 1;
    }

    for (part_num, range) in &ranges {
        // 跳过已上传的分片（断点续传）
        if completed_parts.contains(part_num) {
            continue;
        }

        // 检查 STS 凭证是否即将过期（提前 5 分钟）
        if let Some(deadline_ms) = token_deadline_ms {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            if now_ms >= deadline_ms {
                return Err("token_expired".to_string());
            }
        }

        // 检查暂停/取消信号
        {
            let signal = rx.borrow().clone();
            match signal {
                UploadSignal::Paused => {
                    // 等待恢复
                    loop {
                        rx.changed()
                            .await
                            .map_err(|e| format!("信号通道关闭: {}", e))?;
                        let new_signal = rx.borrow().clone();
                        match new_signal {
                            UploadSignal::Running => break,
                            UploadSignal::Cancelled => {
                                // 取消时清理 OSS 分片上传
                                let _ = client
                                    .abort_multipart_uploads(
                                        &bucket,
                                        &object,
                                        &current_oss_upload_id,
                                    )
                                    .await;
                                return Err("upload_cancelled".to_string());
                            }
                            UploadSignal::Paused => continue,
                        }
                    }
                }
                UploadSignal::Cancelled => {
                    let _ = client
                        .abort_multipart_uploads(&bucket, &object, &current_oss_upload_id)
                        .await;
                    return Err("upload_cancelled".to_string());
                }
                UploadSignal::Running => {}
            }
        }

        let upload_data = UploadPartRequest {
            part_number: *part_num,
            upload_id: current_oss_upload_id.clone(),
        };

        let upload_result = client
            .upload_part_from_file(&bucket, &object, &file_path, range.clone(), upload_data)
            .await
            .map_err(|e| format!("上传分片 {} 失败: {}", part_num, e))?;

        upload_results.push((*part_num, upload_result.etag));
        uploaded_size += range.end - range.start;

        // 发送进度事件
        let _ = app.emit(
            "upload-progress",
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

    // 按分片号排序
    upload_results.sort_by_key(|(n, _)| *n);

    // 完成分片上传（带 callback）
    let complete_request = CompleteMultipartUploadRequest {
        upload_id: current_oss_upload_id,
        parts: upload_results,
    };

    // 构建带 callback 的 options
    let options = build_complete_options(&callback, &callback_var);

    let _resp = client
        .complete_multipart_uploads(&bucket, &object, complete_request, options)
        .await
        .map_err(|e| format!("完成分片上传失败: {}", e))?;

    // 发送完成事件
    let _ = app.emit(
        "upload-progress",
        UploadProgressEvent {
            upload_id: upload_id.clone(),
            uploaded_size: file_size,
            total_size: file_size,
            part_number: total_parts,
            total_parts,
            status: "complete".to_string(),
        },
    );

    // 返回 OSS 响应体
    Ok("ok".to_string())
}

/// 解析 115 回调 JSON 字符串构造 ali-oss-rs Callback 结构体
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

/// 构建 CompleteMultipartUploadOptions（带回调）
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

/// 构建 PutObjectOptions（带回调）
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

async fn simple_upload(
    app: &AppHandle,
    upload_id: &str,
    file_path: &str,
    file_size: u64,
    bucket: &str,
    object: &str,
    client: &ali_oss_rs::Client,
    callback: &str,
    callback_var: &str,
) -> Result<String, String> {
    let options = build_put_options(callback, callback_var);

    client
        .put_object_from_file(bucket, object, file_path, options)
        .await
        .map_err(|e| format!("上传文件失败: {}", e))?;

    // 发送完成事件
    let _ = app.emit(
        "upload-progress",
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

/// 恢复上传任务信号为运行状态
#[tauri::command]
pub fn resume_upload(upload_id: String) -> Result<(), String> {
    let signals = UPLOAD_SIGNALS.lock().unwrap();
    if let Some(tx) = signals.get(&upload_id) {
        tx.send(UploadSignal::Running)
            .map_err(|e| format!("发送恢复信号失败: {}", e))?;
        Ok(())
    } else {
        Err("未找到上传任务".to_string())
    }
}

/// 本地文件信息
#[derive(serde::Serialize, Clone)]
pub struct LocalFileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// 递归扫描本地目录下所有文件
#[tauri::command]
pub async fn scan_directory(dir_path: String) -> Result<Vec<LocalFileInfo>, String> {
    tokio::task::spawn_blocking(move || {
        let mut result = Vec::new();
        scan_dir_recursive(&dir_path, &mut result)?;
        Ok(result)
    })
    .await
    .map_err(|e| format!("扫描目录失败: {}", e))?
}

fn scan_dir_recursive(dir_path: &str, result: &mut Vec<LocalFileInfo>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir_path).map_err(|e| format!("读取目录失败 {}: {}", dir_path, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录条目失败: {}", e))?;
        let metadata = entry
            .metadata()
            .map_err(|e| format!("获取文件元数据失败: {}", e))?;
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();
        let name = entry.file_name().to_string_lossy().to_string();

        if metadata.is_dir() {
            scan_dir_recursive(&path_str, result)?;
        } else {
            result.push(LocalFileInfo {
                path: path_str,
                name,
                size: metadata.len(),
                is_dir: false,
            });
        }
    }

    Ok(())
}

/// 获取单个文件的大小
#[tauri::command]
pub async fn get_file_size(file_path: String) -> Result<u64, String> {
    tokio::task::spawn_blocking(move || {
        let metadata =
            std::fs::metadata(&file_path).map_err(|e| format!("获取文件信息失败: {}", e))?;
        Ok(metadata.len())
    })
    .await
    .map_err(|e| format!("获取文件大小失败: {}", e))?
}
