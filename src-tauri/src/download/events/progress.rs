use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::time::{Duration, interval};

use super::super::types::{SegmentStatus, TaskStatus};

/// 下载进度事件。
///
/// 下载过程中每 500ms 发射一次，用于刷新前端速度和剩余时间。
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgressEvent {
    pub task_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    /// 平滑后的下载速度，单位 bytes/sec。
    pub speed: f64,
    /// 预计剩余秒数；速度为 0 时为 None。
    pub eta_secs: Option<f64>,
}

/// 分片状态变更事件。
///
/// 分片状态切换时立即发射，避免前端依赖批量进度事件感知状态变化。
#[derive(Debug, Clone, Serialize)]
pub struct DownloadSegmentEvent {
    pub task_id: String,
    pub segment_index: u16,
    pub status: SegmentStatus,
    pub downloaded: u64,
}

/// 任务状态变更事件。
///
/// 任务进入完成、失败、暂停等终态时立即发射。
#[derive(Debug, Clone, Serialize)]
pub struct DownloadTaskEvent {
    pub task_id: String,
    pub status: TaskStatus,
}

/// 基于 EMA 的速度平滑器。
pub struct SpeedCalculator {
    alpha: f64,
    smoothed_speed: f64,
    last_bytes: u64,
    last_time: std::time::Instant,
}

impl SpeedCalculator {
    #[allow(dead_code)]
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha,
            smoothed_speed: 0.0,
            last_bytes: 0,
            last_time: std::time::Instant::now(),
        }
    }

    /// 创建速度计算器并以指定的已传输字节数为基准。
    ///
    /// 断点续传时使用，避免首次 tick 将历史累计量当作瞬时增量产生虚假速度尖峰。
    pub fn with_initial_bytes(alpha: f64, initial_bytes: u64) -> Self {
        Self {
            alpha,
            smoothed_speed: 0.0,
            last_bytes: initial_bytes,
            last_time: std::time::Instant::now(),
        }
    }

    /// 使用当前累计下载字节数更新速度，并返回平滑后的 bytes/sec。
    pub fn update(&mut self, current_bytes: u64) -> f64 {
        let elapsed = self.last_time.elapsed().as_secs_f64();

        // 避免在极小时间间隔内放大瞬时速度噪声。
        if elapsed < 0.001 {
            return self.smoothed_speed;
        }

        let delta_bytes = current_bytes.saturating_sub(self.last_bytes);
        let raw_speed = delta_bytes as f64 / elapsed;

        if self.smoothed_speed == 0.0 {
            // 首次更新直接采用瞬时速度，避免从 0 逐步爬升造成显示滞后。
            self.smoothed_speed = raw_speed;
        } else {
            self.smoothed_speed = self.alpha * raw_speed + (1.0 - self.alpha) * self.smoothed_speed;
        }

        self.last_bytes = current_bytes;
        self.last_time = std::time::Instant::now();

        self.smoothed_speed
    }

    /// 返回当前平滑速度，单位 bytes/sec。
    #[allow(dead_code)]
    pub fn speed(&self) -> f64 {
        self.smoothed_speed
    }

    /// 根据剩余字节数估算剩余秒数；速度为 0 时返回 None。
    pub fn eta(&self, remaining_bytes: u64) -> Option<f64> {
        if self.smoothed_speed > 0.0 {
            Some(remaining_bytes as f64 / self.smoothed_speed)
        } else {
            None
        }
    }
}

pub fn emit_progress(app: &AppHandle, event: &DownloadProgressEvent) {
    let _ = app.emit("download:progress", event);
}

pub fn emit_segment_status(app: &AppHandle, event: &DownloadSegmentEvent) {
    let _ = app.emit("download:segment-status", event);
}

pub fn emit_task_status(app: &AppHandle, event: &DownloadTaskEvent) {
    let _ = app.emit("download:task-status", event);
}

/// 下载地址失效事件。
///
/// 前端收到该事件后需要重新获取 URL 并调用 download_provide_url 回传。
#[derive(Debug, Clone, Serialize)]
pub struct UrlExpiredEvent {
    pub task_id: String,
    pub pick_code: String,
}

pub fn emit_url_expired(app: &AppHandle, event: &UrlExpiredEvent) {
    let _ = app.emit("download:url-expired", event);
}

/// download:progress 批量事件中的单项进度快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressItem {
    pub task_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed: f64,
    pub eta_secs: Option<f64>,
    pub status: String,
    pub name: String,
    // 文件夹聚合字段，仅文件夹任务需要。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_folder: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_files: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_files: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_files: Option<i64>,
}

/// 活跃下载任务的进度快照注册表。
pub struct ProgressRegistry {
    entries: Mutex<HashMap<String, ProgressItem>>,
}

impl ProgressRegistry {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// 由下载引擎调用，更新单个任务的最新进度快照。
    pub fn update(&self, item: ProgressItem) {
        self.entries
            .lock()
            .unwrap()
            .insert(item.task_id.clone(), item);
    }

    /// 由下载引擎调用，在任务完成或失败后移除快照。
    pub fn remove(&self, task_id: &str) {
        self.entries.lock().unwrap().remove(task_id);
    }

    /// 返回当前所有活跃任务的快照副本，供聚合与事件发射使用。
    pub fn snapshot(&self) -> Vec<ProgressItem> {
        self.entries.lock().unwrap().values().cloned().collect()
    }
}

/// 进度聚合循环。
///
/// 每 500ms 读取一次活跃任务快照，并补充文件夹聚合项后批量发射 download:progress。
pub async fn progress_loop(
    registry: Arc<ProgressRegistry>,
    folder_aggregator: Arc<super::folder::FolderAggregator>,
    app: AppHandle,
) {
    let mut ticker = interval(Duration::from_millis(500));
    loop {
        ticker.tick().await;
        let mut items = registry.snapshot();
        // 追加文件夹级聚合进度项。
        let folder_items = folder_aggregator.aggregate(&items);
        items.extend(folder_items);
        if !items.is_empty() {
            let _ = app.emit("download:progress", &items);
        }
    }
}
