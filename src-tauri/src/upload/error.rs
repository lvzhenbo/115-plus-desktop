//! 上传域统一错误定义。
//!
//! 这一层只负责表达“上传子系统内部”发生了什么，不关心前端提示文案或 Tauri
//! command 返回值应该长什么样；上层调度器会根据场景决定如何消费这些领域错误。

/// 上传流程内部使用的统一错误枚举。
#[derive(Debug, thiserror::Error)]
pub(crate) enum UploadError {
    /// 上传信号表所在的互斥锁已损坏，说明运行时状态不可继续信任。
    #[error("上传任务状态不可用：内部同步对象已损坏")]
    SignalRegistryPoisoned,
    /// 标准文件 I/O 失败，附带操作名称和目标路径，便于排查本地文件问题。
    #[error("{action}失败：{path}，{source}")]
    Io {
        action: &'static str,
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// 非 I/O 的普通业务错误，通常来自第三方库、参数校验或执行器返回值。
    #[error("{action}失败：{detail}")]
    Message {
        action: &'static str,
        detail: String,
    },
    /// 运行时试图操作一个不存在或已结束的上传任务。
    #[error("未找到上传任务：{0}")]
    UploadNotFound(String),
    /// STS 凭证接近过期，当前上传需要中止并重新申请凭证。
    #[error("上传凭证即将过期，请刷新凭证后重试")]
    TokenExpired,
    /// 运行中的上传被显式暂停，交由上层调度器决定何时重入。
    #[error("上传已暂停")]
    Paused,
    /// 运行中的上传被显式取消，调用方通常需要清理本地状态。
    #[error("上传已取消")]
    Cancelled,
}

/// 上传模块内部统一使用的结果别名。
pub(crate) type UploadResult<T> = Result<T, UploadError>;

/// 构造带路径上下文的 I/O 错误。
pub(crate) fn io_error(
    action: &'static str,
    path: impl Into<String>,
    source: std::io::Error,
) -> UploadError {
    UploadError::Io {
        action,
        path: path.into(),
        source,
    }
}

/// 构造通用文本错误，适合包装第三方库错误或业务校验失败信息。
pub(crate) fn message_error(action: &'static str, detail: impl ToString) -> UploadError {
    UploadError::Message {
        action,
        detail: detail.to_string(),
    }
}
