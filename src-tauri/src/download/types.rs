use std::fmt;

use serde::{Deserialize, Serialize};

/// 最小分片大小阈值 1MB
pub const MIN_SEGMENT_SIZE: u64 = 1024 * 1024;

/// 默认分片数，与 aria2 配置一致
pub const DEFAULT_SEGMENT_COUNT: u16 = 16;

/// 下载配置。
///
/// 目前只暴露前端可调的分片数和全局限速；连接并发由队列层统一调度。
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    /// 文件拆分的总分片数，对应 aria2 的 split 概念。
    pub split: u16,
    /// 全局下载速度上限，单位为 bytes/sec；0 表示不限速。
    pub speed_limit: u64,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            split: DEFAULT_SEGMENT_COUNT, // 16
            speed_limit: 0,
        }
    }
}

/// 下载任务中止原因。
///
/// 和普通错误分开建模，便于队列层准确区分“用户操作”“运行时中断”和“真实失败”。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskAbortReason {
    Paused,
    Cancelled,
    SignalChannelClosed,
    UrlChannelClosed,
    ReadTimeout {
        seconds: u64,
    },
    UrlRefreshTimeout {
        seconds: u64,
    },
    UrlRefreshExhausted {
        max_attempts: u32,
    },
    SemaphoreClosed,
    DownloadFailed,
    SegmentSizeMismatch {
        segment_index: u16,
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for TaskAbortReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Paused => write!(f, "任务已暂停"),
            Self::Cancelled => write!(f, "任务已取消"),
            Self::SignalChannelClosed => write!(f, "任务控制通道已关闭"),
            Self::UrlChannelClosed => write!(f, "下载地址更新通道已关闭"),
            Self::ReadTimeout { seconds } => {
                write!(f, "下载流读取超时（{} 秒）", seconds)
            }
            Self::UrlRefreshTimeout { seconds } => {
                write!(f, "等待下载地址刷新超时（{} 秒）", seconds)
            }
            Self::UrlRefreshExhausted { max_attempts } => {
                write!(f, "下载地址刷新次数已耗尽（最多 {} 次）", max_attempts)
            }
            Self::SemaphoreClosed => write!(f, "下载并发控制器已关闭"),
            Self::DownloadFailed => write!(f, "下载任务执行失败"),
            Self::SegmentSizeMismatch {
                segment_index,
                expected,
                actual,
            } => write!(
                f,
                "分片 {} 字节数不匹配，预期 {} 字节，实际 {} 字节",
                segment_index, expected, actual
            ),
        }
    }
}

/// 下载引擎错误类型。
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("文件 I/O 错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("磁盘空间不足：需要 {needed} 字节，可用 {available} 字节")]
    InsufficientDiskSpace { needed: u64, available: u64 },
    #[error("未找到文件：{0}")]
    FileNotFound(String),
    #[error("HTTP 请求失败：{0}")]
    Http(#[from] reqwest::Error),
    #[error("HTTP 状态异常（{status}）：{message}")]
    HttpStatus { status: u16, message: String },
    #[error("下载地址已失效（HTTP {status}）：{message}")]
    UrlExpired { status: u16, message: String },
    #[error("CDN 返回限流响应（HTTP 403）")]
    CdnRateLimit,
    #[error("{0}")]
    TaskAborted(TaskAbortReason),
    #[error("文件完整性校验失败：{0}")]
    VerificationFailed(String),
    #[error("异步任务执行失败：{0}")]
    JoinError(String),
}

impl DownloadError {
    pub fn is_paused(&self) -> bool {
        matches!(self, Self::TaskAborted(TaskAbortReason::Paused))
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::TaskAborted(TaskAbortReason::Cancelled))
    }

    pub fn is_user_abort(&self) -> bool {
        self.is_paused() || self.is_cancelled()
    }
}

/// 分片生命周期状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SegmentStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
    Paused,
    /// 分片已被重分配，其剩余范围已拆分给新子分片
    Reallocated,
}

/// 单个分片的下载状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub index: u16,
    pub start: u64,
    /// 分片结束偏移，包含该字节位置。
    pub end: u64,
    pub status: SegmentStatus,
    /// 当前分片已落盘的字节数。
    pub downloaded: u64,
}

impl Segment {
    #[allow(dead_code)]
    pub fn len(&self) -> u64 {
        self.end - self.start + 1
    }
}

/// HEAD 探测结果。
#[derive(Debug, Clone)]
pub struct RangeInfo {
    /// 服务器是否支持 Accept-Ranges: bytes。
    pub supports_range: bool,
    /// 服务器返回的 ETag，用于续传前校验文件是否发生变化。
    pub etag: Option<String>,
}

/// 下载任务状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Active,
    Paused,
    Complete,
    Error,
    /// SHA1 校验失败
    VerifyFailed,
}

/// 下载任务元数据。
#[derive(Debug, Clone, Serialize)]
pub struct DownloadTask {
    /// 任务唯一标识，使用 UUID v4 生成。
    pub task_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub save_path: String,
    pub url: String,
    /// 115 文件 pick_code，用于下载地址失效后重新换取新地址。
    pub pick_code: String,
    /// HEAD 探测得到的 ETag，用于续传前判断远端文件是否变化。
    pub etag: Option<String>,
    /// 115 接口返回的预期 SHA1，用于下载完成后的完整性校验。
    pub expected_sha1: Option<String>,
    pub segments: Vec<Segment>,
    pub status: TaskStatus,
    /// 任务创建时间，Unix 毫秒时间戳。
    pub created_at: u64,
}

/// 分片进度更新消息，通过 channel 发送给刷盘与事件聚合循环。
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub task_id: String,
    pub segment_index: u16,
    pub downloaded: u64,
}
