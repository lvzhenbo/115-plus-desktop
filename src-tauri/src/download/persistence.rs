use std::collections::HashMap;
use std::io::Write;
use std::sync::Mutex;

use super::types::{DownloadError, Segment, SegmentStatus};

/// .oofp 进度文件管理器。
///
/// 每个下载文件对应一个 `<save_path>.oofp` 进度文件，
/// 存储任务元数据和分片进度，语义上类似 aria2 的 `.aria2` 断点文件。
/// 普通删除任务时应保留它，确保后续重新添加同一路径任务时可以断点续传；
/// 只有下载完成或明确判定旧断点失效时才删除。
///
/// 内部会缓存 OofpData，避免每次更新都重新读取和解析文件。
/// batch_update_downloaded 每 500ms 调用一次，缓存可以显著减少热路径上的文件 I/O。
pub struct ProgressFile {
    /// 运行时映射：task_id → save_path，用于从任务 id 定位 .oofp 文件。
    paths: Mutex<HashMap<String, String>>,
    /// 内存缓存：save_path → OofpData，避免重复读取和解析。
    cache: Mutex<HashMap<String, OofpData>>,
}

/// 未完成的下载任务（含分片信息）
#[derive(Debug, Clone, serde::Serialize)]
pub struct IncompleteTask {
    pub task_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub save_path: String,
    pub url: String,
    pub etag: Option<String>,
    pub pick_code: String,
    pub expected_sha1: Option<String>,
    pub created_at: u64,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Copy)]
pub struct TaskProgressSnapshot {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub progress: f64,
}

/// .oofp 文件的内存表示。
#[derive(Debug, Clone)]
struct OofpData {
    task_id: String,
    file_name: String,
    file_size: u64,
    url: String,
    etag: Option<String>,
    pick_code: String,
    expected_sha1: Option<String>,
    status: String,
    created_at: u64,
    segments: Vec<Segment>,
}

fn status_to_str(status: &SegmentStatus) -> &'static str {
    match status {
        SegmentStatus::Pending => "pending",
        SegmentStatus::Downloading => "downloading",
        SegmentStatus::Completed => "completed",
        SegmentStatus::Failed => "failed",
        SegmentStatus::Paused => "paused",
        SegmentStatus::Reallocated => "reallocated",
    }
}

fn str_to_status(s: &str) -> SegmentStatus {
    match s {
        "downloading" => SegmentStatus::Downloading,
        "completed" => SegmentStatus::Completed,
        "failed" => SegmentStatus::Failed,
        "paused" => SegmentStatus::Paused,
        "reallocated" => SegmentStatus::Reallocated,
        _ => SegmentStatus::Pending,
    }
}

/// 计算 .oofp 进度文件路径。
fn oofp_path(save_path: &str) -> String {
    format!("{}.oofp", save_path)
}

/// 将 OofpData 序列化为 .oofp 文件内容。
fn serialize_oofp(data: &OofpData) -> String {
    let mut buf = String::with_capacity(512);
    buf.push_str("[oofp v1]\n");
    buf.push_str(&format!("task_id={}\n", data.task_id));
    buf.push_str(&format!("file_name={}\n", data.file_name));
    buf.push_str(&format!("file_size={}\n", data.file_size));
    buf.push_str(&format!("url={}\n", data.url));
    buf.push_str(&format!("etag={}\n", data.etag.as_deref().unwrap_or("")));
    buf.push_str(&format!("pick_code={}\n", data.pick_code));
    buf.push_str(&format!(
        "expected_sha1={}\n",
        data.expected_sha1.as_deref().unwrap_or("")
    ));
    buf.push_str(&format!("status={}\n", data.status));
    buf.push_str(&format!("created_at={}\n", data.created_at));
    buf.push_str("[segments]\n");
    for seg in &data.segments {
        buf.push_str(&format!(
            "{} {} {} {} {}\n",
            seg.index,
            seg.start,
            seg.end,
            seg.downloaded,
            status_to_str(&seg.status),
        ));
    }
    buf
}

/// 从 .oofp 文件内容反序列化为结构化数据。
fn deserialize_oofp(content: &str) -> Result<OofpData, DownloadError> {
    let mut task_id = String::new();
    let mut file_name = String::new();
    let mut file_size: u64 = 0;
    let mut url = String::new();
    let mut etag: Option<String> = None;
    let mut pick_code = String::new();
    let mut expected_sha1: Option<String> = None;
    let mut status = "active".to_string();
    let mut created_at: u64 = 0;
    let mut segments: Vec<Segment> = Vec::new();
    let mut in_segments = false;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[oofp v1]" {
            continue;
        }
        if line == "[segments]" {
            in_segments = true;
            continue;
        }
        if in_segments {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 5 {
                segments.push(Segment {
                    index: parts[0].parse().unwrap_or(0),
                    start: parts[1].parse().unwrap_or(0),
                    end: parts[2].parse().unwrap_or(0),
                    downloaded: parts[3].parse().unwrap_or(0),
                    status: str_to_status(parts[4]),
                });
            }
        } else if let Some((key, value)) = line.split_once('=') {
            match key {
                "task_id" => task_id = value.to_string(),
                "file_name" => file_name = value.to_string(),
                "file_size" => file_size = value.parse().unwrap_or(0),
                "url" => url = value.to_string(),
                "etag" => {
                    etag = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    }
                }
                "pick_code" => pick_code = value.to_string(),
                "expected_sha1" => {
                    expected_sha1 = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    }
                }
                "status" => status = value.to_string(),
                "created_at" => created_at = value.parse().unwrap_or(0),
                _ => {}
            }
        }
    }

    if task_id.is_empty() {
        return Err(DownloadError::FileNotFound(
            "Invalid .oofp file: missing task_id".to_string(),
        ));
    }

    Ok(OofpData {
        task_id,
        file_name,
        file_size,
        url,
        etag,
        pick_code,
        expected_sha1,
        status,
        created_at,
        segments,
    })
}

/// 原子写入 .oofp 文件（写入 .tmp 后 rename，防止崩溃导致文件损坏）
fn atomic_write(path: &str, content: &str) -> Result<(), DownloadError> {
    let tmp_path = format!("{}.tmp", path);
    let mut file = std::fs::File::create(&tmp_path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

/// 从 .oofp 文件读取数据
fn read_oofp(save_path: &str) -> Result<OofpData, DownloadError> {
    let path = oofp_path(save_path);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| DownloadError::FileNotFound(format!("Cannot read {}: {}", path, e)))?;
    deserialize_oofp(&content)
}

impl ProgressFile {
    pub fn new() -> Self {
        Self {
            paths: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// 带缓存的 update_oofp — 读取用内存缓存，写入同时更新缓存+磁盘
    ///
    /// 热路径优化: 缓存命中时跳过文件读取+解析，只做序列化+原子写入
    fn update_cached<F>(&self, save_path: &str, updater: F) -> Result<(), DownloadError>
    where
        F: FnOnce(&mut OofpData),
    {
        let content = {
            let mut cache = self.cache.lock().unwrap();
            if let Some(data) = cache.get_mut(save_path) {
                // 缓存命中: 直接修改内存数据并序列化
                updater(data);
                serialize_oofp(data)
            } else {
                drop(cache);
                // 缓存未命中: 从磁盘读取，更新后写入缓存
                let mut data = read_oofp(save_path)?;
                updater(&mut data);
                let content = serialize_oofp(&data);
                self.cache
                    .lock()
                    .unwrap()
                    .insert(save_path.to_string(), data);
                content
            }
        };
        atomic_write(&oofp_path(save_path), &content)
    }

    /// 注册 task_id → save_path 映射
    fn register_path(&self, task_id: &str, save_path: &str) {
        self.paths
            .lock()
            .unwrap()
            .insert(task_id.to_string(), save_path.to_string());
    }

    /// 通过 task_id 获取 save_path
    fn get_save_path(&self, task_id: &str) -> Option<String> {
        self.paths.lock().unwrap().get(task_id).cloned()
    }

    /// 注销 task_id 映射
    fn unregister_path(&self, task_id: &str) {
        self.paths.lock().unwrap().remove(task_id);
    }

    /// 保存任务元数据 — 创建 .oofp 文件并写入缓存
    pub fn save_task(
        &self,
        task_id: &str,
        file_name: &str,
        file_size: u64,
        save_path: &str,
        url: &str,
        etag: Option<&str>,
        pick_code: &str,
        expected_sha1: Option<&str>,
        created_at: u64,
    ) -> Result<(), DownloadError> {
        self.register_path(task_id, save_path);
        let data = OofpData {
            task_id: task_id.to_string(),
            file_name: file_name.to_string(),
            file_size,
            url: url.to_string(),
            etag: etag.map(|s| s.to_string()),
            pick_code: pick_code.to_string(),
            expected_sha1: expected_sha1.map(|s| s.to_string()),
            status: "active".to_string(),
            created_at,
            segments: Vec::new(),
        };
        let content = serialize_oofp(&data);
        // 写入缓存
        self.cache
            .lock()
            .unwrap()
            .insert(save_path.to_string(), data);
        atomic_write(&oofp_path(save_path), &content)
    }

    /// 保存所有分片到 .oofp 文件
    pub fn save_segments(&self, task_id: &str, segments: &[Segment]) -> Result<(), DownloadError> {
        let save_path = self
            .get_save_path(task_id)
            .ok_or_else(|| DownloadError::FileNotFound(format!("No path for task {}", task_id)))?;
        self.update_cached(&save_path, |data| {
            data.segments = segments.to_vec();
        })
    }

    /// 批量更新分片已下载字节数（每 500ms 调用，热路径）
    pub fn batch_update_downloaded(
        &self,
        updates: &[(String, u16, u64)],
    ) -> Result<(), DownloadError> {
        if updates.is_empty() {
            return Ok(());
        }
        let task_id = &updates[0].0;
        let save_path = self
            .get_save_path(task_id)
            .ok_or_else(|| DownloadError::FileNotFound(format!("No path for task {}", task_id)))?;
        self.update_cached(&save_path, |data| {
            for (_, index, downloaded) in updates {
                if let Some(seg) = data.segments.iter_mut().find(|s| s.index == *index) {
                    seg.downloaded = *downloaded;
                }
            }
        })
    }

    /// 即时更新分片状态
    pub fn update_segment_status(
        &self,
        task_id: &str,
        segment_index: u16,
        status: &SegmentStatus,
        downloaded: u64,
    ) -> Result<(), DownloadError> {
        let save_path = self
            .get_save_path(task_id)
            .ok_or_else(|| DownloadError::FileNotFound(format!("No path for task {}", task_id)))?;
        self.update_cached(&save_path, |data| {
            if let Some(seg) = data.segments.iter_mut().find(|s| s.index == segment_index) {
                seg.status = status.clone();
                seg.downloaded = downloaded;
            }
        })
    }

    /// 更新任务状态
    pub fn update_task_status(&self, task_id: &str, status: &str) -> Result<(), DownloadError> {
        let save_path = self
            .get_save_path(task_id)
            .ok_or_else(|| DownloadError::FileNotFound(format!("No path for task {}", task_id)))?;
        self.update_cached(&save_path, |data| {
            data.status = status.to_string();
        })
    }

    /// 更新存储的 URL (per URL-03)
    /// URL 刷新成功后调用，将新 URL 持久化到 .oofp，防止崩溃恢复时用过期 URL
    pub fn update_task_url(&self, task_id: &str, url: &str) -> Result<(), DownloadError> {
        let save_path = self
            .get_save_path(task_id)
            .ok_or_else(|| DownloadError::FileNotFound(format!("No path for task {}", task_id)))?;
        self.update_cached(&save_path, |data| {
            data.url = url.to_string();
        })
    }

    /// 插入重分配产生的新子分片
    pub fn insert_segments(
        &self,
        task_id: &str,
        segments: &[Segment],
    ) -> Result<(), DownloadError> {
        let save_path = self
            .get_save_path(task_id)
            .ok_or_else(|| DownloadError::FileNotFound(format!("No path for task {}", task_id)))?;
        self.update_cached(&save_path, |data| {
            for new_seg in segments {
                if let Some(existing) = data.segments.iter_mut().find(|s| s.index == new_seg.index)
                {
                    *existing = new_seg.clone();
                } else {
                    data.segments.push(new_seg.clone());
                }
            }
        })
    }

    /// 从 .oofp 文件加载任务并写入缓存（通过 save_path）
    pub fn load_task(&self, save_path: &str) -> Result<IncompleteTask, DownloadError> {
        let data = read_oofp(save_path)?;
        self.register_path(&data.task_id, save_path);
        // 写入缓存，后续更新无需重新读取文件
        let task = IncompleteTask {
            task_id: data.task_id.clone(),
            file_name: data.file_name.clone(),
            file_size: data.file_size,
            save_path: save_path.to_string(),
            url: data.url.clone(),
            etag: data.etag.clone(),
            pick_code: data.pick_code.clone(),
            expected_sha1: data.expected_sha1.clone(),
            created_at: data.created_at,
            segments: data.segments.clone(),
        };
        self.cache
            .lock()
            .unwrap()
            .insert(save_path.to_string(), data);
        Ok(task)
    }

    /// 将现有 .oofp 绑定到新任务 ID，允许删除任务后重新添加时继续续传。
    ///
    /// 这使 `.oofp` 的行为更接近 `.aria2`：任务记录可以删除，但断点文件仍可复用。
    pub fn rebind_task(
        &self,
        save_path: &str,
        old_task_id: &str,
        new_task_id: &str,
        file_name: &str,
        url: &str,
        pick_code: &str,
        expected_sha1: Option<&str>,
    ) -> Result<(), DownloadError> {
        self.update_cached(save_path, |data| {
            data.task_id = new_task_id.to_string();
            data.file_name = file_name.to_string();
            data.url = url.to_string();
            data.pick_code = pick_code.to_string();
            if let Some(expected_sha1) = expected_sha1 {
                data.expected_sha1 = Some(expected_sha1.to_string());
            }
            data.status = "paused".to_string();
        })?;

        if old_task_id != new_task_id {
            self.unregister_path(old_task_id);
        }
        self.register_path(new_task_id, save_path);
        Ok(())
    }

    /// 删除断点文件 — 删除 .oofp 文件、清除缓存并注销映射。
    ///
    /// 仅用于下载完成、校验/ETag 判定需要重下等场景；
    /// 普通“删除任务”不应调用它，否则会失去断点续传能力。
    pub fn delete_task(&self, task_id: &str) -> Result<(), DownloadError> {
        if let Some(save_path) = self.get_save_path(task_id) {
            let path = oofp_path(&save_path);
            let _ = std::fs::remove_file(&path);
            self.cache.lock().unwrap().remove(&save_path);
            self.unregister_path(task_id);
        }
        Ok(())
    }

    /// 读取任务当前已下载字节与进度百分比。
    pub fn get_task_progress(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskProgressSnapshot>, DownloadError> {
        let Some(save_path) = self.get_save_path(task_id) else {
            return Ok(None);
        };

        let data = if let Some(data) = self.cache.lock().unwrap().get(&save_path).cloned() {
            data
        } else {
            let data = read_oofp(&save_path)?;
            self.cache
                .lock()
                .unwrap()
                .insert(save_path.clone(), data.clone());
            data
        };

        let downloaded_bytes = data.segments.iter().map(|segment| segment.downloaded).sum();
        let progress = if data.file_size == 0 {
            0.0
        } else {
            ((downloaded_bytes as f64 / data.file_size as f64) * 100.0).clamp(0.0, 100.0)
        };

        Ok(Some(TaskProgressSnapshot {
            downloaded_bytes,
            total_bytes: data.file_size,
            progress,
        }))
    }
}
