//! 上传子系统入口。
//!
//! 这个模块本身不承载具体业务逻辑，只负责把上传能力拆分为多个职责清晰的子模块：
//! - `store`: 上传任务持久化
//! - `sync`: 顶层任务状态同步与前端事件推送
//! - `api`: 前后端上传 API 桥接、115 协议载荷定义与上传前计划协商
//! - `queue`: 上传调度、重试与运行态控制
//! - `folder`: 文件夹上传编排与父任务状态汇总
//! - `local`: 本地文件哈希、扫描与元数据读取
//! - `oss`: 真实的 OSS 上传执行器
//! - `control`: 运行中任务的暂停/恢复/取消信号管理
//! - `error`: 上传域统一错误定义

pub mod api;
pub mod control;
pub mod error;
mod folder;
pub mod local;
pub mod oss;
pub mod progress;
pub mod queue;
pub mod store;
pub mod sync;

use tauri::App;

/// 上传模块初始化阶段可能出现的错误。
#[derive(Debug, thiserror::Error)]
pub enum UploadInitError {
    #[error("初始化上传存储失败：{0}")]
    InitStore(#[from] store::UploadInitError),
    #[error("初始化上传队列失败：{0}")]
    InitQueue(#[from] queue::UploadQueueInitError),
}

/// 初始化上传模块。
///
/// 初始化顺序固定为“先存储、再同步、再桥接、后队列”：
/// - 队列恢复未完成任务时依赖存储已经可用
/// - 状态同步器也必须在调度器启动前就绪
pub fn init(app: &App) -> Result<(), UploadInitError> {
    store::init(app)?;
    sync::init(app);
    api::init(app);
    queue::init(app)?;
    Ok(())
}
