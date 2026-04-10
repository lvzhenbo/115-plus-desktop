use std::collections::HashMap;
use std::sync::Mutex;

use super::progress::ProgressItem;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct FolderProgressSnapshot {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub progress: f64,
}

/// 文件夹内部聚合状态
pub struct FolderState {
    pub name: String,
    pub total_files: i64,
    pub total_bytes: u64,
    pub completed_files: i64,
    pub failed_files: i64,
    /// 已完成子任务的总字节数（用于进度计算，避免与活跃快照双重计数）
    pub completed_bytes: u64,
}

/// 文件夹进度聚合器 — 追踪文件夹下载状态并从子任务快照计算聚合进度 (per D-04)
pub struct FolderAggregator {
    /// parent_gid → FolderState
    folders: Mutex<HashMap<String, FolderState>>,
    /// child task_id → parent_gid（用于 progress_loop 分组）
    child_parents: Mutex<HashMap<String, String>>,
    /// child task_id → 最后一次已知已下载字节数（含 paused/error 等非活跃状态）
    child_downloaded: Mutex<HashMap<String, u64>>,
}

impl FolderAggregator {
    pub fn new() -> Self {
        Self {
            folders: Mutex::new(HashMap::new()),
            child_parents: Mutex::new(HashMap::new()),
            child_downloaded: Mutex::new(HashMap::new()),
        }
    }

    /// 注册文件夹（download_enqueue_folder 调用）。
    pub fn register_folder(
        &self,
        parent_gid: &str,
        name: &str,
        total_files: i64,
        total_bytes: u64,
    ) {
        self.folders.lock().unwrap().insert(
            parent_gid.to_string(),
            FolderState {
                name: name.to_string(),
                total_files,
                total_bytes,
                completed_files: 0,
                failed_files: 0,
                completed_bytes: 0,
            },
        );
    }

    /// 注册子任务到文件夹（download_enqueue_folder 会对每个子任务调用）。
    pub fn register_child(&self, child_id: &str, parent_gid: &str) {
        self.child_parents
            .lock()
            .unwrap()
            .insert(child_id.to_string(), parent_gid.to_string());
        self.child_downloaded
            .lock()
            .unwrap()
            .entry(child_id.to_string())
            .or_insert(0);
    }

    /// 更新子任务的最近下载字节数，供 paused/error 状态下继续聚合文件夹进度。
    pub fn update_child_progress(&self, child_id: &str, downloaded_bytes: u64) {
        self.child_downloaded
            .lock()
            .unwrap()
            .insert(child_id.to_string(), downloaded_bytes);
    }

    /// 子任务完成时递增完成计数 + 累加已完成字节
    pub fn increment_completed(&self, child_id: &str, child_size: u64) {
        let parent = self.child_parents.lock().unwrap().get(child_id).cloned();
        if let Some(parent_gid) = parent {
            if let Some(state) = self.folders.lock().unwrap().get_mut(&parent_gid) {
                state.completed_files += 1;
                state.completed_bytes += child_size;
            }
        }
        self.child_downloaded.lock().unwrap().remove(child_id);
    }

    /// 子任务失败时递增失败计数
    pub fn increment_failed(&self, child_id: &str) {
        let parent = self.child_parents.lock().unwrap().get(child_id).cloned();
        if let Some(parent_gid) = parent {
            if let Some(state) = self.folders.lock().unwrap().get_mut(&parent_gid) {
                state.failed_files += 1;
            }
        }
    }

    /// 移除子任务记录（任务完成/取消后清理）
    pub fn remove_child(&self, child_id: &str) {
        self.child_parents.lock().unwrap().remove(child_id);
        self.child_downloaded.lock().unwrap().remove(child_id);
    }

    /// 移除文件夹（文件夹完成或取消后清理）
    pub fn remove_folder(&self, parent_gid: &str) {
        self.folders.lock().unwrap().remove(parent_gid);
        // 清理所有属于该文件夹的子任务映射及缓存进度。
        let mut child_parents = self.child_parents.lock().unwrap();
        let mut child_downloaded = self.child_downloaded.lock().unwrap();
        child_parents.retain(|child_id, value| {
            let keep = value != parent_gid;
            if !keep {
                child_downloaded.remove(child_id);
            }
            keep
        });
    }

    /// 恢复文件夹的已完成/已失败计数（resume/retry 时使用）
    pub fn restore_counters(
        &self,
        parent_gid: &str,
        completed_files: i64,
        failed_files: i64,
        completed_bytes: u64,
    ) {
        if let Some(state) = self.folders.lock().unwrap().get_mut(parent_gid) {
            state.completed_files = completed_files;
            state.failed_files = failed_files;
            state.completed_bytes = completed_bytes;
        }
    }

    pub fn get_progress(&self, parent_gid: &str) -> Option<FolderProgressSnapshot> {
        let folders = self.folders.lock().unwrap();
        let state = folders.get(parent_gid)?;
        let child_parents = self.child_parents.lock().unwrap();
        let child_downloaded = self.child_downloaded.lock().unwrap();

        let partial_downloaded = child_parents
            .iter()
            .filter(|(_, value)| value.as_str() == parent_gid)
            .map(|(child_id, _)| child_downloaded.get(child_id).copied().unwrap_or(0))
            .sum::<u64>();
        let downloaded_bytes = state.completed_bytes + partial_downloaded;
        let progress = if state.total_bytes == 0 {
            0.0
        } else {
            ((downloaded_bytes as f64 / state.total_bytes as f64) * 100.0).clamp(0.0, 100.0)
        };

        Some(FolderProgressSnapshot {
            downloaded_bytes,
            total_bytes: state.total_bytes,
            progress,
        })
    }

    /// 从子任务快照聚合文件夹进度项 (per D-04, D-05)
    /// 返回文件夹级别的 ProgressItem 列表，追加到 download:progress 数组。
    pub fn aggregate(&self, child_items: &[ProgressItem]) -> Vec<ProgressItem> {
        {
            let mut child_downloaded = self.child_downloaded.lock().unwrap();
            for item in child_items {
                child_downloaded.insert(item.task_id.clone(), item.downloaded_bytes);
            }
        }

        let folders = self.folders.lock().unwrap();
        let child_parents = self.child_parents.lock().unwrap();
        let child_downloaded = self.child_downloaded.lock().unwrap();

        // 按 parent_gid 分组子任务快照
        let mut grouped: HashMap<&str, Vec<&ProgressItem>> = HashMap::new();
        for item in child_items {
            if let Some(parent_gid) = child_parents.get(&item.task_id) {
                grouped.entry(parent_gid.as_str()).or_default().push(item);
            }
        }

        let mut downloaded_by_parent: HashMap<&str, u64> = HashMap::new();
        for (child_id, parent_gid) in child_parents.iter() {
            let downloaded = child_downloaded.get(child_id).copied().unwrap_or(0);
            *downloaded_by_parent.entry(parent_gid.as_str()).or_default() += downloaded;
        }

        let mut result = Vec::new();
        for (parent_gid, state) in folders.iter() {
            let children = grouped.get(parent_gid.as_str());
            let active_speed: f64 = children
                .map(|c| c.iter().map(|i| i.speed).sum())
                .unwrap_or(0.0);
            let partial_downloaded = downloaded_by_parent
                .get(parent_gid.as_str())
                .copied()
                .unwrap_or(0);
            let total_downloaded = state.completed_bytes + partial_downloaded;
            let remaining = state.total_bytes.saturating_sub(total_downloaded);
            let eta = if active_speed > 0.0 {
                Some(remaining as f64 / active_speed)
            } else {
                None
            };

            let has_activity = children.is_some() && !children.unwrap().is_empty();
            let has_progress =
                total_downloaded > 0 || state.completed_files > 0 || state.failed_files > 0;
            if !has_activity && !has_progress {
                continue;
            }

            result.push(ProgressItem {
                task_id: parent_gid.clone(),
                downloaded_bytes: total_downloaded,
                total_bytes: state.total_bytes,
                speed: active_speed,
                eta_secs: eta,
                status: "active".to_string(),
                name: state.name.clone(),
                is_folder: true,
                completed_files: Some(state.completed_files),
                failed_files: Some(state.failed_files),
                total_files: Some(state.total_files),
            });
        }
        result
    }
}
