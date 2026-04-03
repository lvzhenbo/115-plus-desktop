use std::collections::HashMap;
use std::io::Write;
use std::sync::Mutex;

use super::types::{DownloadError, Segment, SegmentStatus};

/// .oofp 进度文件管理器 — 类似 aria2 的 .aria2 控制文件
///
/// 每个下载文件对应一个 `<save_path>.oofp` 进度文件，
/// 存储任务元数据和分片进度。下载完成后自动删除。
pub struct ProgressFile {
    /// 运行时映射: task_id → save_path（用于从 task_id 定位 .oofp 文件）
    paths: Mutex<HashMap<String, String>>,
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

/// .oofp 文件内存表示
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

/// 获取 .oofp 进度文件路径
fn oofp_path(save_path: &str) -> String {
    format!("{}.oofp", save_path)
}

/// 将 OofpData 序列化为 .oofp 文件内容
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

/// 从 .oofp 文件内容反序列化
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

/// 读取 → 修改 → 原子写回
fn update_oofp<F>(save_path: &str, updater: F) -> Result<(), DownloadError>
where
    F: FnOnce(&mut OofpData),
{
    let mut data = read_oofp(save_path)?;
    updater(&mut data);
    let content = serialize_oofp(&data);
    atomic_write(&oofp_path(save_path), &content)
}

impl ProgressFile {
    pub fn new() -> Self {
        Self {
            paths: Mutex::new(HashMap::new()),
        }
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

    /// 保存任务元数据 — 创建 .oofp 文件
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
        atomic_write(&oofp_path(save_path), &content)
    }

    /// 保存所有分片到 .oofp 文件
    pub fn save_segments(&self, task_id: &str, segments: &[Segment]) -> Result<(), DownloadError> {
        let save_path = self
            .get_save_path(task_id)
            .ok_or_else(|| DownloadError::FileNotFound(format!("No path for task {}", task_id)))?;
        update_oofp(&save_path, |data| {
            data.segments = segments.to_vec();
        })
    }

    /// 批量更新分片已下载字节数（per D-04 每 500ms 调用）
    pub fn batch_update_downloaded(
        &self,
        updates: &[(String, u16, u64)],
    ) -> Result<(), DownloadError> {
        if updates.is_empty() {
            return Ok(());
        }
        // 所有 updates 属于同一个 task_id
        let task_id = &updates[0].0;
        let save_path = self
            .get_save_path(task_id)
            .ok_or_else(|| DownloadError::FileNotFound(format!("No path for task {}", task_id)))?;
        update_oofp(&save_path, |data| {
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
        update_oofp(&save_path, |data| {
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
        update_oofp(&save_path, |data| {
            data.status = status.to_string();
        })
    }

    /// 更新存储的 URL (per URL-03)
    pub fn update_task_url(&self, task_id: &str, url: &str) -> Result<(), DownloadError> {
        let save_path = self
            .get_save_path(task_id)
            .ok_or_else(|| DownloadError::FileNotFound(format!("No path for task {}", task_id)))?;
        update_oofp(&save_path, |data| {
            data.url = url.to_string();
        })
    }

    /// 插入重分配产生的新子分片 (per D-12)
    pub fn insert_segments(
        &self,
        task_id: &str,
        segments: &[Segment],
    ) -> Result<(), DownloadError> {
        let save_path = self
            .get_save_path(task_id)
            .ok_or_else(|| DownloadError::FileNotFound(format!("No path for task {}", task_id)))?;
        update_oofp(&save_path, |data| {
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

    /// 从 .oofp 文件加载任务（通过 save_path）
    pub fn load_task(&self, save_path: &str) -> Result<IncompleteTask, DownloadError> {
        let data = read_oofp(save_path)?;
        self.register_path(&data.task_id, save_path);
        Ok(IncompleteTask {
            task_id: data.task_id,
            file_name: data.file_name,
            file_size: data.file_size,
            save_path: save_path.to_string(),
            url: data.url,
            etag: data.etag,
            pick_code: data.pick_code,
            expected_sha1: data.expected_sha1,
            created_at: data.created_at,
            segments: data.segments,
        })
    }

    /// 删除任务 — 删除 .oofp 文件并注销映射
    pub fn delete_task(&self, task_id: &str) -> Result<(), DownloadError> {
        if let Some(save_path) = self.get_save_path(task_id) {
            let path = oofp_path(&save_path);
            let _ = std::fs::remove_file(&path);
            self.unregister_path(task_id);
        }
        Ok(())
    }
}

