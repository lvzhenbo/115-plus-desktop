//! 文件夹上传编排与父任务聚合。
//!
//! 这一层负责两件事：
//! - 把本地目录树展开为父任务 + 子文件任务
//! - 根据子任务状态回写父文件夹任务的聚合进度

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use log::{error, info, warn};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use super::api::{UploadApiResolver, request_create_folder};
use super::local::scan_directory_internal;
use super::progress::UploadProgressRegistry;
use super::queue::{
    PendingTask, UploadQueue, UploadQueueError, get_existing_task, now_ms, safe_delete_task,
    safe_update_task,
};
use super::store::{DbHandle, TaskUpdate, UploadTask};
use super::sync::UploadStateSync;

#[derive(Debug)]
struct LocalFolderFile {
    path: String,
    name: String,
    size: i64,
    relative_path: String,
}

/// 目录收集守卫。
///
/// 一旦文件夹展开流程已经注册为 collecting，就必须在所有退出路径上清理该标记，
/// 否则 pause-all 的完成判断会一直卡住。
struct CollectionGuard<'a> {
    queue: &'a UploadQueue,
    parent_id: &'a str,
}

impl<'a> CollectionGuard<'a> {
    fn start(queue: &'a UploadQueue, parent_id: &'a str) -> Result<Self, UploadQueueError> {
        queue.mark_collection_started(parent_id)?;
        Ok(Self { queue, parent_id })
    }
}

impl Drop for CollectionGuard<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.queue.finish_collection(self.parent_id) {
            warn!(
                "[上传文件夹] 收集收尾失败 parent_id={}: {}",
                self.parent_id, error
            );
        }
    }
}

fn derive_parent_folder_status(
    parent_status: &str,
    completed: i64,
    failed: i64,
    paused: i64,
    active: i64,
    total: i64,
) -> Option<&'static str> {
    if completed + failed == total {
        if failed > 0 {
            Some("error")
        } else {
            Some("complete")
        }
    } else if paused > 0 && active == 0 {
        Some("paused")
    } else if active > 0 {
        if parent_status == "pausing" {
            Some("pausing")
        } else {
            Some("uploading")
        }
    } else {
        None
    }
}

/// 根据全部子任务的状态重算父文件夹任务。
pub(super) async fn sync_parent_folder(
    db: &DbHandle,
    state_sync: &UploadStateSync,
    parent_id: &str,
) {
    let Some(parent) = get_existing_task(db, parent_id).await else {
        return;
    };
    if !parent.is_folder {
        return;
    }

    let Ok(children) = db.get_child_tasks(parent_id.to_string()).await else {
        return;
    };

    if children.is_empty() {
        return;
    }

    let completed = children
        .iter()
        .filter(|task| task.status == "complete")
        .count() as i64;
    let failed = children
        .iter()
        .filter(|task| task.status == "error")
        .count() as i64;
    let paused = children
        .iter()
        .filter(|task| task.status == "paused")
        .count() as i64;
    let active = children
        .iter()
        .filter(|task| {
            matches!(
                task.status.as_str(),
                "pending" | "hashing" | "uploading" | "pausing"
            )
        })
        .count() as i64;

    let total_size: i64 = children.iter().map(|task| task.file_size).sum();
    let completed_size = children.iter().fold(0f64, |sum, task| {
        if task.status == "complete" {
            sum + task.file_size as f64
        } else {
            sum + task.file_size as f64 * (task.progress / 100.0)
        }
    });

    let mut updates = TaskUpdate {
        completed_files: Some(Some(completed)),
        failed_files: Some(Some(failed)),
        total_files: Some(Some(children.len() as i64)),
        file_size: Some(total_size),
        progress: Some(if total_size > 0 {
            (completed_size / total_size as f64 * 10000.0).round() / 100.0
        } else {
            0.0
        }),
        // 速度和 ETA 由 progress_loop 在内存中实时聚合，与下载侧保持一致。
        ..TaskUpdate::default()
    };

    if let Some(status) = derive_parent_folder_status(
        &parent.status,
        completed,
        failed,
        paused,
        active,
        children.len() as i64,
    ) {
        updates.status = Some(status.to_string());
        if status == "error" {
            updates.error_message = Some(Some(format!("{} 个文件上传失败", failed)));
        } else if status == "complete" {
            updates.completed_at = Some(Some(now_ms()));
            updates.error_message = Some(None);
        } else if status == "uploading" || status == "pausing" {
            updates.error_message = Some(None);
            updates.completed_at = Some(None);
        }
    }

    let _ = safe_update_task(db, parent_id.to_string(), updates).await;
    state_sync.notify_state_change();
}

/// 文件夹上传编排流程。
///
/// 这里负责把一个目录树拆成“一个父文件夹任务 + 多个普通文件任务”，并在 115 端按层级
/// 建立对应目录结构。
pub(super) async fn enqueue_folder_impl(
    app: &AppHandle,
    db: &DbHandle,
    state_sync: &UploadStateSync,
    queue: &UploadQueue,
    api_resolver: &Arc<UploadApiResolver>,
    parent_id: String,
    folder_path: String,
    folder_name: String,
    target_cid: String,
    reuse_existing_task: bool,
) -> Result<(), UploadQueueError> {
    if queue.is_folder_paused(&parent_id)? {
        info!(
            "[上传文件夹][{}] 收集前检测到暂停，直接进入 paused",
            parent_id
        );
        let _ = safe_update_task(
            db,
            parent_id,
            TaskUpdate {
                status: Some("paused".to_string()),
                ..TaskUpdate::default()
            },
        )
        .await;
        state_sync.notify_state_change();
        return Ok(());
    }

    info!(
        "[上传文件夹][{}] 开始收集 folder={} target_cid={} reuse_existing={}",
        parent_id, folder_path, target_cid, reuse_existing_task
    );

    if !reuse_existing_task {
        db.insert_task(UploadTask {
            id: parent_id.clone(),
            file_name: folder_name.clone(),
            file_path: folder_path.clone(),
            file_size: 0,
            target_cid: target_cid.clone(),
            target_path: None,
            sha1: None,
            pre_sha1: None,
            pick_code: None,
            status: "pending".to_string(),
            progress: 0.0,
            error_message: None,
            created_at: Some(now_ms()),
            completed_at: None,
            is_folder: true,
            parent_id: None,
            total_files: Some(0),
            completed_files: Some(0),
            failed_files: Some(0),
            oss_bucket: None,
            oss_object: None,
            oss_endpoint: None,
            callback: None,
            callback_var: None,
            uploaded_size: 0,
            file_id: None,
            oss_upload_id: None,
        })
        .await?;
    } else {
        let _ = safe_update_task(
            db,
            parent_id.clone(),
            TaskUpdate {
                status: Some("pending".to_string()),
                progress: Some(0.0),
                error_message: Some(None),
                completed_at: Some(None),
                completed_files: Some(Some(0)),
                failed_files: Some(Some(0)),
                total_files: Some(Some(0)),
                ..TaskUpdate::default()
            },
        )
        .await;
    }
    state_sync.notify_state_change();

    let _collection_guard = CollectionGuard::start(queue, &parent_id)?;

    let all_files = match collect_local_folder_files(&folder_path).await {
        Ok(files) => files,
        Err(err) => {
            error!(
                "[上传文件夹][{}] 本地扫描失败 path={}: {}",
                parent_id, folder_path, err
            );
            let _ = safe_update_task(
                db,
                parent_id.clone(),
                TaskUpdate {
                    status: Some("error".to_string()),
                    error_message: Some(Some("收集文件列表失败".to_string())),
                    ..TaskUpdate::default()
                },
            )
            .await;
            state_sync.notify_state_change();
            return Err(err);
        }
    };

    if stop_collection_if_cancelled(queue, &parent_id)? {
        return Ok(());
    }

    let total_files_count = all_files.len();
    let total_size = all_files.iter().map(|file| file.size).sum::<i64>();
    info!(
        "[上传文件夹][{}] 本地扫描完成 files={} total_size={}B",
        parent_id, total_files_count, total_size
    );

    // 注册文件夹总量到进度注册表，使文件夹 ETA 基于全量剩余字节计算，与下载侧一致。
    let progress_registry = app.state::<Arc<UploadProgressRegistry>>();
    progress_registry.register_folder(parent_id.clone(), total_size.max(0) as u64);

    let _ = safe_update_task(
        db,
        parent_id.clone(),
        TaskUpdate {
            total_files: Some(Some(total_files_count as i64)),
            file_size: Some(total_size),
            status: Some("pending".to_string()),
            ..TaskUpdate::default()
        },
    )
    .await;
    state_sync.notify_state_change();

    let root_folder_cid = resolve_root_folder_cid(
        app,
        db,
        api_resolver,
        &parent_id,
        &folder_name,
        &target_cid,
        reuse_existing_task,
    )
    .await?;

    info!(
        "[上传文件夹][{}] 远端根目录已就绪 cid={}",
        parent_id, root_folder_cid
    );

    let _ = safe_update_task(
        db,
        parent_id.clone(),
        TaskUpdate {
            file_id: Some(Some(root_folder_cid.clone())),
            ..TaskUpdate::default()
        },
    )
    .await;

    if all_files.is_empty() {
        info!(
            "[上传文件夹][{}] 空文件夹无需创建子任务，直接完成",
            parent_id
        );
        let _ = safe_update_task(
            db,
            parent_id.clone(),
            TaskUpdate {
                status: Some("complete".to_string()),
                progress: Some(100.0),
                completed_at: Some(Some(now_ms())),
                file_id: Some(Some(root_folder_cid)),
                ..TaskUpdate::default()
            },
        )
        .await;
        state_sync.notify_state_change();
        return Ok(());
    }

    let mut dir_paths = HashSet::new();
    for file in &all_files {
        let parts: Vec<&str> = file.relative_path.split('/').collect();
        for depth in 1..parts.len() {
            dir_paths.insert(parts[..depth].join("/"));
        }
    }
    let mut sorted_dirs: Vec<String> = dir_paths.into_iter().collect();
    sorted_dirs.sort_by(|left, right| {
        left.split('/')
            .count()
            .cmp(&right.split('/').count())
            .then_with(|| left.cmp(right))
    });

    if !sorted_dirs.is_empty() {
        info!(
            "[上传文件夹][{}] 开始创建远端子目录 count={}",
            parent_id,
            sorted_dirs.len()
        );
    }

    let mut dir_cid_map = HashMap::new();
    for dir_path in sorted_dirs {
        if stop_collection_if_cancelled(queue, &parent_id)? {
            return Ok(());
        }

        let parts: Vec<&str> = dir_path.split('/').collect();
        let dir_name = parts.last().unwrap_or(&dir_path.as_str()).to_string();
        let parent_rel_path = parts[..parts.len().saturating_sub(1)].join("/");
        let parent_cid = if parent_rel_path.is_empty() {
            root_folder_cid.clone()
        } else {
            dir_cid_map.get(&parent_rel_path).cloned().ok_or_else(|| {
                UploadQueueError::Internal(format!(
                    "找不到父目录 {} 对应的远端 cid",
                    parent_rel_path
                ))
            })?
        };
        let cid =
            request_create_folder(app, api_resolver, &parent_id, dir_name, parent_cid).await?;
        dir_cid_map.insert(dir_path, cid);
    }

    let mut enqueued_children = 0usize;
    let mut paused_children = 0usize;

    for file in all_files {
        if stop_collection_if_cancelled(queue, &parent_id)? {
            return Ok(());
        }

        let file_id = format!("upload-{}-{}", now_ms(), Uuid::new_v4());
        let child_should_start_paused = queue.is_folder_paused(&parent_id)?;
        let parent_dir = file
            .relative_path
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_string())
            .unwrap_or_default();
        let file_target_cid = if parent_dir.is_empty() {
            root_folder_cid.clone()
        } else {
            dir_cid_map.get(&parent_dir).cloned().ok_or_else(|| {
                UploadQueueError::Internal(format!("找不到目录 {} 对应的远端 cid", parent_dir))
            })?
        };

        db.insert_task(UploadTask {
            id: file_id.clone(),
            file_name: file.name,
            file_path: file.path,
            file_size: file.size,
            target_cid: file_target_cid,
            target_path: None,
            sha1: None,
            pre_sha1: None,
            pick_code: None,
            status: if child_should_start_paused {
                "paused".to_string()
            } else {
                "pending".to_string()
            },
            progress: 0.0,
            error_message: None,
            created_at: Some(now_ms()),
            completed_at: None,
            is_folder: false,
            parent_id: Some(parent_id.clone()),
            total_files: None,
            completed_files: None,
            failed_files: None,
            oss_bucket: None,
            oss_object: None,
            oss_endpoint: None,
            callback: None,
            callback_var: None,
            uploaded_size: 0,
            file_id: None,
            oss_upload_id: None,
        })
        .await?;

        if queue.is_collection_cancelled(&parent_id)? {
            let _ = safe_delete_task(db, &file_id).await;
            return Ok(());
        }

        if queue.is_folder_paused(&parent_id)? {
            let _ = safe_update_task(
                db,
                file_id,
                TaskUpdate {
                    status: Some("paused".to_string()),
                    ..TaskUpdate::default()
                },
            )
            .await;
            paused_children += 1;
            continue;
        }

        queue
            .enqueue(PendingTask {
                id: file_id,
                parent_id: Some(parent_id.clone()),
            })
            .await?;
        enqueued_children += 1;
    }

    info!(
        "[上传文件夹][{}] 子任务已准备 total={} enqueued={} paused={}",
        parent_id, total_files_count, enqueued_children, paused_children
    );

    if queue.is_folder_paused(&parent_id)? {
        info!("[上传文件夹][{}] 收集完成，但父任务保持 paused", parent_id);
        let _ = safe_update_task(
            db,
            parent_id.clone(),
            TaskUpdate {
                status: Some("paused".to_string()),
                file_id: Some(Some(root_folder_cid)),
                ..TaskUpdate::default()
            },
        )
        .await;
        state_sync.notify_state_change();
        return Ok(());
    }

    let _ = safe_update_task(
        db,
        parent_id.clone(),
        TaskUpdate {
            status: Some("uploading".to_string()),
            file_id: Some(Some(root_folder_cid)),
            ..TaskUpdate::default()
        },
    )
    .await;
    state_sync.notify_state_change();
    info!("[上传文件夹][{}] 收集完成，父任务进入 uploading", parent_id);
    Ok(())
}

async fn resolve_root_folder_cid(
    app: &AppHandle,
    db: &DbHandle,
    api_resolver: &Arc<UploadApiResolver>,
    parent_id: &str,
    folder_name: &str,
    target_cid: &str,
    reuse_existing_task: bool,
) -> Result<String, UploadQueueError> {
    if reuse_existing_task {
        if let Some(task) = get_existing_task(db, parent_id).await {
            if let Some(file_id) = task.file_id {
                info!(
                    "[上传文件夹][{}] 复用已有远端根目录 cid={}",
                    parent_id, file_id
                );
                return Ok(file_id);
            }
        }
    }

    info!(
        "[上传文件夹][{}] 创建远端根目录 name={} parent_cid={}",
        parent_id, folder_name, target_cid
    );

    request_create_folder(
        app,
        api_resolver,
        parent_id,
        folder_name.to_string(),
        target_cid.to_string(),
    )
    .await
    .map_err(|err| UploadQueueError::Internal(format!("创建根文件夹失败: {}", err)))
}

fn normalize_relative_path(base_dir: &str, full_path: &str, fallback_name: &str) -> String {
    Path::new(full_path)
        .strip_prefix(Path::new(base_dir))
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| fallback_name.to_string())
}

async fn collect_local_folder_files(
    folder_path: &str,
) -> Result<Vec<LocalFolderFile>, UploadQueueError> {
    let files = scan_directory_internal(folder_path.to_string())
        .await
        .map_err(|err| UploadQueueError::Internal(err.to_string()))?;

    let mut result = Vec::new();
    for file in files.into_iter().filter(|item| !item.is_dir) {
        result.push(LocalFolderFile {
            path: file.path.clone(),
            name: file.name.clone(),
            size: file.size as i64,
            relative_path: normalize_relative_path(folder_path, &file.path, &file.name),
        });
    }
    Ok(result)
}

fn stop_collection_if_cancelled(
    queue: &UploadQueue,
    parent_id: &str,
) -> Result<bool, UploadQueueError> {
    if queue.is_collection_cancelled(parent_id)? {
        return Ok(true);
    }

    Ok(false)
}
