//! 上传进度聚合与速度计算。
//!
//! 与下载侧 `events::progress` 保持一致的架构：
//! - 速度基于 EMA 平滑，存储在内存中而非数据库
//! - 每 500ms 聚合一次快照并推送 `upload:progress` 事件
//! - 文件夹速度由子任务速度实时汇总

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::time::{Duration, interval};

/// `upload:progress` 批量事件中的单项进度快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadProgressItem {
    pub task_id: String,
    pub uploaded_size: u64,
    pub total_size: u64,
    /// 平滑后的上传速度，单位 bytes/sec。
    pub speed: f64,
    /// 预计剩余秒数；速度为 0 时为 None。
    pub eta_secs: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_folder: bool,
}

/// 基于 EMA 的速度平滑器，与下载 SpeedCalculator 完全一致。
struct SpeedCalculator {
    alpha: f64,
    smoothed_speed: f64,
    last_bytes: u64,
    last_time: std::time::Instant,
}

impl SpeedCalculator {
    fn new(alpha: f64) -> Self {
        Self {
            alpha,
            smoothed_speed: 0.0,
            last_bytes: 0,
            last_time: std::time::Instant::now(),
        }
    }

    /// 使用当前累计上传字节数更新速度，并返回平滑后的 bytes/sec。
    fn update(&mut self, current_bytes: u64) -> f64 {
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

    /// 根据剩余字节数估算剩余秒数；速度为 0 时返回 None。
    fn eta(&self, remaining_bytes: u64) -> Option<f64> {
        if self.smoothed_speed > 0.0 {
            Some(remaining_bytes as f64 / self.smoothed_speed)
        } else {
            None
        }
    }
}

/// 内部注册表条目。
struct ProgressEntry {
    uploaded_size: u64,
    total_size: u64,
    parent_id: Option<String>,
    speed_calc: SpeedCalculator,
}

/// 文件夹级别的聚合状态，与下载侧 `FolderAggregator` 中的 `FolderState` 对应。
struct FolderState {
    /// 文件夹内所有子文件的总字节数。
    total_bytes: u64,
    /// 已完成子文件的累计字节数（不再出现在 entries 中的部分）。
    completed_bytes: u64,
}

/// 活跃上传任务的进度快照注册表。
///
/// 与下载侧 `ProgressRegistry` + `FolderAggregator` 对应，速度在内存中计算，不持久化到数据库。
pub struct UploadProgressRegistry {
    entries: Mutex<HashMap<String, ProgressEntry>>,
    /// parent_id → FolderState，追踪文件夹总量与已完成量。
    folders: Mutex<HashMap<String, FolderState>>,
}

impl UploadProgressRegistry {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            folders: Mutex::new(HashMap::new()),
        }
    }

    /// 注册文件夹及其总字节数（扫描完成后调用）。
    pub fn register_folder(&self, parent_id: String, total_bytes: u64) {
        self.folders.lock().unwrap().insert(
            parent_id,
            FolderState {
                total_bytes,
                completed_bytes: 0,
            },
        );
    }

    /// 子任务完成时累加已完成字节数。
    pub fn increment_completed(&self, parent_id: &str, child_size: u64) {
        if let Some(state) = self.folders.lock().unwrap().get_mut(parent_id) {
            state.completed_bytes += child_size;
        }
    }

    /// 文件夹任务结束后移除聚合状态。
    pub fn remove_folder(&self, parent_id: &str) {
        self.folders.lock().unwrap().remove(parent_id);
    }

    /// 由上传引擎调用，更新单个任务的最新已上传字节数。
    pub fn update(
        &self,
        task_id: String,
        uploaded_size: u64,
        total_size: u64,
        parent_id: Option<String>,
    ) {
        let mut map = self.entries.lock().unwrap();
        let entry = map.entry(task_id).or_insert_with(|| {
            // 用当前已上传字节数初始化 last_bytes，避免断点续传时
            // 首次 tick 将历史累计量当作瞬时增量，产生虚假速度尖峰。
            let mut speed_calc = SpeedCalculator::new(0.3);
            speed_calc.last_bytes = uploaded_size;
            ProgressEntry {
                uploaded_size,
                total_size,
                parent_id: parent_id.clone(),
                speed_calc,
            }
        });
        entry.uploaded_size = uploaded_size;
        entry.total_size = total_size;
    }

    /// 由上传引擎调用，在任务完成或失败后移除快照。
    pub fn remove(&self, task_id: &str) {
        self.entries.lock().unwrap().remove(task_id);
    }

    /// 每 500ms 调用一次，计算速度并生成快照列表。
    fn tick(&self) -> Vec<UploadProgressItem> {
        let mut map = self.entries.lock().unwrap();
        let folders = self.folders.lock().unwrap();
        let mut items = Vec::new();
        // parent_id → (sum_speed, sum_active_uploaded, sum_active_total)
        let mut folder_agg: HashMap<String, (f64, u64, u64)> = HashMap::new();

        for (task_id, entry) in map.iter_mut() {
            let speed = entry.speed_calc.update(entry.uploaded_size);
            let remaining = entry.total_size.saturating_sub(entry.uploaded_size);
            let eta = entry.speed_calc.eta(remaining);

            items.push(UploadProgressItem {
                task_id: task_id.clone(),
                uploaded_size: entry.uploaded_size,
                total_size: entry.total_size,
                speed,
                eta_secs: eta,
                is_folder: false,
            });

            // 聚合到父文件夹
            if let Some(ref pid) = entry.parent_id {
                let agg = folder_agg.entry(pid.clone()).or_insert((0.0, 0, 0));
                agg.0 += speed;
                agg.1 += entry.uploaded_size;
                agg.2 += entry.total_size;
            }
        }

        // 生成文件夹级聚合项，与下载侧 FolderAggregator::aggregate 保持一致：
        // ETA = (folder_total_bytes - completed_bytes - active_uploaded) / total_speed
        for (parent_id, (total_speed, active_uploaded, active_total)) in folder_agg {
            if let Some(state) = folders.get(&parent_id) {
                let total_uploaded = state.completed_bytes + active_uploaded;
                let remaining = state.total_bytes.saturating_sub(total_uploaded);
                let eta = if total_speed > 0.0 {
                    Some(remaining as f64 / total_speed)
                } else {
                    None
                };
                items.push(UploadProgressItem {
                    task_id: parent_id,
                    uploaded_size: total_uploaded,
                    total_size: state.total_bytes,
                    speed: total_speed,
                    eta_secs: eta,
                    is_folder: true,
                });
            } else {
                // 回退：文件夹扫描尚未完成，仅以活跃子任务的剩余量估算
                let active_remaining = active_total.saturating_sub(active_uploaded);
                let eta = if total_speed > 0.0 {
                    Some(active_remaining as f64 / total_speed)
                } else {
                    None
                };
                items.push(UploadProgressItem {
                    task_id: parent_id,
                    uploaded_size: active_uploaded,
                    total_size: active_total,
                    speed: total_speed,
                    eta_secs: eta,
                    is_folder: true,
                });
            }
        }

        items
    }
}

/// 进度聚合循环。
///
/// 每 500ms 读取一次活跃任务快照，计算平滑速度后批量发射 `upload:progress`。
pub async fn progress_loop(registry: std::sync::Arc<UploadProgressRegistry>, app: AppHandle) {
    let mut ticker = interval(Duration::from_millis(500));
    loop {
        ticker.tick().await;
        let items = registry.tick();
        if !items.is_empty() {
            let _ = app.emit("upload:progress", &items);
        }
    }
}
