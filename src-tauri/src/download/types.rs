use serde::{Deserialize, Serialize};

/// 最小分片大小阈值 1MB (per D-03)
pub const MIN_SEGMENT_SIZE: u64 = 1024 * 1024;

/// 默认分片数，与 aria2 配置一致 (per D-05)
pub const DEFAULT_SEGMENT_COUNT: u16 = 16;

/// 下载配置参数 (per D-01, D-02)
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    /// 文件拆分的总分片数 (aria2 split)
    pub split: u16,
    /// 每服务器最大并行连接数 (aria2 max-connection-per-server)
    pub max_connections_per_server: u16,
    /// 全局下载速度上限 (bytes/sec), 0 = 不限速 (per D-04)
    pub speed_limit: u64,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            split: DEFAULT_SEGMENT_COUNT, // 16
            max_connections_per_server: 16,
            speed_limit: 0,
        }
    }
}

/// 下载引擎错误类型 (per D-08)
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Insufficient disk space: need {needed} bytes, available {available} bytes")]
    InsufficientDiskSpace { needed: u64, available: u64 },
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Unexpected HTTP status {status}: {message}")]
    HttpStatus { status: u16, message: String },
    #[error("URL expired (HTTP {status}): {message}")]
    UrlExpired { status: u16, message: String },
    #[error("CDN rate limited (HTTP 403)")]
    CdnRateLimit,
    #[error("Task aborted: {0}")]
    TaskAborted(String),
    #[error("Task join error: {0}")]
    JoinError(String),
}

/// 分片状态 (per D-11)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SegmentStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
    Paused,
    /// 分片已被重分配，其剩余范围已拆分给新子分片 (per D-09)
    Reallocated,
}

/// 单个分片数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub index: u16,
    pub start: u64,
    /// inclusive byte offset
    pub end: u64,
    pub status: SegmentStatus,
    /// bytes downloaded so far
    pub downloaded: u64,
}

impl Segment {
    #[allow(dead_code)]
    pub fn len(&self) -> u64 {
        self.end - self.start + 1
    }
}

/// HEAD 请求检测结果 — Range 支持情况和服务器元数据
#[derive(Debug, Clone)]
pub struct RangeInfo {
    /// 服务器是否支持 Accept-Ranges: bytes
    pub supports_range: bool,
    /// ETag 值，用于 Phase 4 断点续传的 If-Range 验证
    pub etag: Option<String>,
}

/// 任务状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Active,
    Paused,
    Complete,
    Error,
    /// SHA1 校验失败 (per D-07)
    VerifyFailed,
}

/// 下载任务元数据 (per D-10)
#[derive(Debug, Clone, Serialize)]
pub struct DownloadTask {
    /// UUID v4
    pub task_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub save_path: String,
    pub url: String,
    /// pick_code for URL expiration refresh (per URL-02)
    pub pick_code: String,
    /// ETag from HEAD response, used for If-Range validation in Phase 4
    pub etag: Option<String>,
    /// Expected SHA1 from 115 API, for post-download verification (per D-05)
    pub expected_sha1: Option<String>,
    pub segments: Vec<Segment>,
    pub status: TaskStatus,
    /// unix timestamp millis
    pub created_at: u64,
}

/// 分片进度更新消息，通过 channel 发送给 flush collector
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub task_id: String,
    pub segment_index: u16,
    pub downloaded: u64,
}
