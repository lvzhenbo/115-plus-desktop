//! 上传调度中心。
//!
//! 这一层把前端触发的“上传动作”组织成后台可恢复、可暂停、可观察的任务流。
//! 它本身不直接实现数据库或 115 API，而是分别依赖：
//! - `store.rs` 持久化任务和同步列表状态
//! - `oss.rs` 执行真实的 OSS 上传
//! - 前端通过 `upload:api-needed` 事件代为调用 115 接口并回填结果
//!
//! 整体采用“单调度循环 + 多执行任务”的模型：
//! - 调度循环串行处理等待队列、控制命令和任务完成回调
//! - 执行任务只关注单个文件的一次上传生命周期
//! - 文件夹任务先展开目录结构，再把子文件重新投递为普通文件任务

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::DateTime;
use log::{error, info, warn};
use serde::Deserialize;
use tauri::async_runtime::JoinHandle;
use tauri::{App, AppHandle, Manager};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Duration;
use uuid::Uuid;

use super::api::{
    UploadApiRequest, UploadApiResolver, UploadCallback, UploadTokenData, prepare_upload_plan,
};
use super::control::{upload_cancel, upload_pause};
use super::error::UploadError;
use super::folder::{enqueue_folder_impl, sync_parent_folder};
use super::local::compute_file_hash_internal;
use super::oss::{OssUploadInitEvent, UploadHooks, UploadProgressEvent, upload_to_oss_internal};
use super::store::{DbHandle, TaskUpdate, UploadStoreError, UploadTask};
use super::sync::UploadStateSync;

const ERR_QUEUE_CHANNEL_CLOSED: &str = "上传队列不可用：调度通道已关闭";
const ERR_COLLECTION_STATE_POISONED: &str = "上传收集状态异常：内部锁已损坏";
const STATUS_PAUSED: &str = "paused";
const STATUS_PAUSING: &str = "pausing";

/// 基于 EMA 的上传速度平滑器，与下载 SpeedCalculator 保持一致。
struct UploadSpeedCalculator {
    alpha: f64,
    smoothed_speed: f64,
    last_bytes: u64,
    last_time: std::time::Instant,
}

impl UploadSpeedCalculator {
    fn new(alpha: f64) -> Self {
        Self {
            alpha,
            smoothed_speed: 0.0,
            last_bytes: 0,
            last_time: std::time::Instant::now(),
        }
    }

    fn update(&mut self, current_bytes: u64) -> f64 {
        let elapsed = self.last_time.elapsed().as_secs_f64();
        if elapsed < 0.001 {
            return self.smoothed_speed;
        }
        let delta_bytes = current_bytes.saturating_sub(self.last_bytes);
        let raw_speed = delta_bytes as f64 / elapsed;
        if self.smoothed_speed == 0.0 {
            self.smoothed_speed = raw_speed;
        } else {
            self.smoothed_speed = self.alpha * raw_speed + (1.0 - self.alpha) * self.smoothed_speed;
        }
        self.last_bytes = current_bytes;
        self.last_time = std::time::Instant::now();
        self.smoothed_speed
    }

    fn eta(&self, remaining_bytes: u64) -> Option<f64> {
        if self.smoothed_speed > 0.0 {
            Some(remaining_bytes as f64 / self.smoothed_speed)
        } else {
            None
        }
    }
}

/// 上传调度层统一对外暴露的错误类型。
#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "type", content = "message")]
pub enum UploadQueueError {
    #[error("数据库错误: {0}")]
    Db(String),
    #[error("未找到: {0}")]
    NotFound(String),
    #[error("内部错误: {0}")]
    Internal(String),
}

impl From<UploadStoreError> for UploadQueueError {
    fn from(value: UploadStoreError) -> Self {
        match value {
            UploadStoreError::DbError(message) => Self::Db(message),
            UploadStoreError::NotFound(message) => Self::NotFound(message),
            UploadStoreError::Internal(message) => Self::Internal(message),
        }
    }
}

impl From<serde_json::Error> for UploadQueueError {
    fn from(value: serde_json::Error) -> Self {
        Self::Internal(value.to_string())
    }
}

/// 前端调用 `upload_enqueue_files` 时传入的最小文件描述。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalUploadFileInput {
    pub path: String,
    pub name: String,
    pub size: i64,
}

/// 调度器内部排队的最小任务单元。
///
/// 这里只保留 id 和父任务关系，避免把完整任务快照长时间缓存在线程内存里。
#[derive(Debug, Clone)]
pub(super) struct PendingTask {
    pub(super) id: String,
    pub(super) parent_id: Option<String>,
}

/// 单个执行任务在运行过程中可收到的控制信号。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskSignal {
    Running,
    Paused,
    Cancelled,
}

/// 发给调度循环的控制命令。
///
/// 这些命令统一在同一个事件循环里串行消费，用来保证等待队列和活跃任务表的一致性。
enum ControlCommand {
    Pause {
        id: String,
    },
    Resume {
        task: PendingTask,
        front: bool,
    },
    Remove {
        id: String,
    },
    PauseFolder {
        parent_id: String,
    },
    RemoveFolder {
        parent_id: String,
    },
    PauseAll {
        completion: oneshot::Sender<()>,
    },
    ResumeAll {
        completion: oneshot::Sender<Result<(), UploadQueueError>>,
    },
}

/// 单个执行任务结束后回传给调度器的结果。
enum TaskCompletion {
    Completed { id: String },
    Failed { id: String, error: String },
    Paused { id: String },
    Cancelled { id: String },
}

impl TaskCompletion {
    fn id(&self) -> &str {
        match self {
            Self::Completed { id }
            | Self::Failed { id, .. }
            | Self::Paused { id }
            | Self::Cancelled { id } => id,
        }
    }
}

/// 进入 `oss.rs` 前整理好的目标信息。
#[derive(Debug)]
struct PreparedUploadTarget {
    bucket: String,
    object: String,
    callback: UploadCallback,
}

/// 一次“全部暂停”操作在点击瞬间锁定的目标集合。
///
/// 只追踪当次需要暂停到位的活跃任务和正在收集的文件夹，避免把之后新进入队列的任务
/// 也错误地纳入本次全部暂停。
#[derive(Default)]
struct PauseAllSnapshot {
    active_task_ids: HashSet<String>,
    collecting_folder_ids: HashSet<String>,
}

/// 上传调度器的共享句柄。
///
/// 真正的调度状态都运行在 `queue_loop` 里，这个结构只是 Tauri command 与初始化逻辑持有
/// 的入口。
#[derive(Clone)]
pub struct UploadQueue {
    enqueue_tx: mpsc::Sender<PendingTask>,
    control_tx: mpsc::Sender<ControlCommand>,
    max_concurrent: Arc<AtomicUsize>,
    max_retry: Arc<AtomicUsize>,
    collecting_folders: Arc<Mutex<HashSet<String>>>,
    cancelled_folder_collections: Arc<Mutex<HashSet<String>>>,
    paused_folders: Arc<Mutex<HashSet<String>>>,
    pause_all_requested: Arc<AtomicUsize>,
}

impl UploadQueue {
    /// 启动调度循环并返回对外共享的句柄。
    fn start(
        app: AppHandle,
        db: DbHandle,
        state_sync: UploadStateSync,
        api_resolver: Arc<UploadApiResolver>,
    ) -> Self {
        let (enqueue_tx, enqueue_rx) = mpsc::channel::<PendingTask>(256);
        let (control_tx, control_rx) = mpsc::channel::<ControlCommand>(128);
        let (completion_tx, completion_rx) = mpsc::channel::<TaskCompletion>(256);
        let max_concurrent = Arc::new(AtomicUsize::new(3));
        let max_retry = Arc::new(AtomicUsize::new(3));
        let collecting_folders = Arc::new(Mutex::new(HashSet::new()));
        let cancelled_folder_collections = Arc::new(Mutex::new(HashSet::new()));
        let paused_folders = Arc::new(Mutex::new(HashSet::new()));
        let pause_all_requested = Arc::new(AtomicUsize::new(0));

        let queue = Self {
            enqueue_tx,
            control_tx,
            max_concurrent,
            max_retry,
            collecting_folders,
            cancelled_folder_collections,
            paused_folders,
            pause_all_requested,
        };

        tauri::async_runtime::spawn(queue_loop(
            queue.clone(),
            enqueue_rx,
            control_rx,
            completion_rx,
            completion_tx,
            queue.max_concurrent.clone(),
            queue.max_retry.clone(),
            queue.collecting_folders.clone(),
            queue.paused_folders.clone(),
            queue.pause_all_requested.clone(),
            app,
            db,
            state_sync,
            api_resolver,
        ));

        queue
    }

    /// 向调度循环发送控制命令。
    async fn send_control(&self, command: ControlCommand) -> Result<(), UploadQueueError> {
        self.control_tx
            .send(command)
            .await
            .map_err(|_| UploadQueueError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    /// 向等待队列压入一个待执行任务。
    pub(super) async fn enqueue(&self, task: PendingTask) -> Result<(), UploadQueueError> {
        self.enqueue_tx
            .send(task)
            .await
            .map_err(|_| UploadQueueError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    async fn pause(&self, id: String) -> Result<(), UploadQueueError> {
        self.send_control(ControlCommand::Pause { id }).await
    }

    async fn resume(&self, task: PendingTask, front: bool) -> Result<(), UploadQueueError> {
        self.send_control(ControlCommand::Resume { task, front })
            .await
    }

    async fn remove(&self, id: String) -> Result<(), UploadQueueError> {
        self.send_control(ControlCommand::Remove { id }).await
    }

    async fn pause_folder(&self, parent_id: String) -> Result<(), UploadQueueError> {
        self.mark_folder_paused(&parent_id)?;
        self.send_control(ControlCommand::PauseFolder { parent_id })
            .await
    }

    async fn remove_folder(&self, parent_id: String) -> Result<(), UploadQueueError> {
        self.send_control(ControlCommand::RemoveFolder { parent_id })
            .await
    }

    async fn pause_all(&self) -> Result<(), UploadQueueError> {
        let collecting_folder_ids: Vec<String> = {
            Self::collection_lock(&self.collecting_folders)?
                .iter()
                .cloned()
                .collect()
        };
        {
            let mut paused = Self::collection_lock(&self.paused_folders)?;
            for parent_id in collecting_folder_ids {
                paused.insert(parent_id);
            }
        }
        self.pause_all_requested.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        if let Err(error) = self
            .send_control(ControlCommand::PauseAll { completion: tx })
            .await
        {
            self.pause_all_requested.fetch_sub(1, Ordering::SeqCst);
            return Err(error);
        }

        rx.await.map_err(|_| {
            self.pause_all_requested.fetch_sub(1, Ordering::SeqCst);
            UploadQueueError::Internal("上传队列不可用：暂停确认通道已关闭".into())
        })
    }

    async fn resume_all(&self) -> Result<(), UploadQueueError> {
        if self.pause_all_requested.load(Ordering::SeqCst) > 0 {
            return Err(UploadQueueError::Internal(
                "全部暂停还没完成，请稍后再试".into(),
            ));
        }

        let (tx, rx) = oneshot::channel();
        self.send_control(ControlCommand::ResumeAll { completion: tx })
            .await?;
        rx.await
            .map_err(|_| UploadQueueError::Internal("上传队列不可用：恢复确认通道已关闭".into()))?
    }

    fn set_max_concurrent(&self, n: usize) {
        self.max_concurrent.store(n.clamp(1, 8), Ordering::SeqCst);
    }

    fn set_max_retry(&self, n: usize) {
        self.max_retry.store(n.min(10), Ordering::SeqCst);
    }

    fn collection_lock(
        set: &Mutex<HashSet<String>>,
    ) -> Result<std::sync::MutexGuard<'_, HashSet<String>>, UploadQueueError> {
        set.lock()
            .map_err(|_| UploadQueueError::Internal(ERR_COLLECTION_STATE_POISONED.into()))
    }

    pub(super) fn mark_collection_started(&self, parent_id: &str) -> Result<(), UploadQueueError> {
        Self::collection_lock(&self.collecting_folders)?.insert(parent_id.to_string());
        Self::collection_lock(&self.cancelled_folder_collections)?.remove(parent_id);
        Ok(())
    }

    pub(super) fn finish_collection(&self, parent_id: &str) -> Result<(), UploadQueueError> {
        Self::collection_lock(&self.collecting_folders)?.remove(parent_id);
        Self::collection_lock(&self.cancelled_folder_collections)?.remove(parent_id);
        Ok(())
    }

    fn cancel_collection(&self, parent_id: &str) -> Result<(), UploadQueueError> {
        Self::collection_lock(&self.cancelled_folder_collections)?.insert(parent_id.to_string());
        Ok(())
    }

    fn mark_folder_paused(&self, parent_id: &str) -> Result<(), UploadQueueError> {
        Self::collection_lock(&self.paused_folders)?.insert(parent_id.to_string());
        Ok(())
    }

    fn clear_folder_pause(&self, parent_id: &str) -> Result<(), UploadQueueError> {
        Self::collection_lock(&self.paused_folders)?.remove(parent_id);
        Ok(())
    }

    pub(super) fn is_collection_cancelled(
        &self,
        parent_id: &str,
    ) -> Result<bool, UploadQueueError> {
        Ok(Self::collection_lock(&self.cancelled_folder_collections)?.contains(parent_id))
    }

    pub(super) fn is_folder_paused(&self, parent_id: &str) -> Result<bool, UploadQueueError> {
        Ok(Self::collection_lock(&self.paused_folders)?.contains(parent_id))
    }

    pub(super) fn is_collecting(&self, parent_id: &str) -> Result<bool, UploadQueueError> {
        Ok(Self::collection_lock(&self.collecting_folders)?.contains(parent_id))
    }
}

async fn queue_loop(
    queue: UploadQueue,
    mut enqueue_rx: mpsc::Receiver<PendingTask>,
    mut control_rx: mpsc::Receiver<ControlCommand>,
    mut completion_rx: mpsc::Receiver<TaskCompletion>,
    completion_tx: mpsc::Sender<TaskCompletion>,
    max_concurrent: Arc<AtomicUsize>,
    max_retry: Arc<AtomicUsize>,
    collecting_folders: Arc<Mutex<HashSet<String>>>,
    paused_folders: Arc<Mutex<HashSet<String>>>,
    pause_all_requested: Arc<AtomicUsize>,
    app: AppHandle,
    db: DbHandle,
    state_sync: UploadStateSync,
    api_resolver: Arc<UploadApiResolver>,
) {
    // `waiting` 保存尚未启动的任务；`active`/`signals`/`active_parents` 分别追踪
    // 运行中的任务句柄、控制信号和父子关系，便于控制命令与聚合逻辑共享同一份状态。
    let mut waiting: VecDeque<PendingTask> = VecDeque::new();
    let mut active: HashMap<String, JoinHandle<()>> = HashMap::new();
    let mut signals: HashMap<String, watch::Sender<TaskSignal>> = HashMap::new();
    let mut active_parents: HashMap<String, Option<String>> = HashMap::new();
    let mut pause_all_waiters: Vec<oneshot::Sender<()>> = Vec::new();
    let mut pause_all_snapshot: Option<PauseAllSnapshot> = None;

    // 应用重启后，数据库里遗留的进行中任务统一回落到 paused，由用户显式恢复。
    recover_tasks(&db, &state_sync).await;

    loop {
        pause_blocked_waiting_tasks(&mut waiting, &db, &state_sync, &paused_folders).await;

        // 只要还有并发余量，就持续从等待队列拉起任务执行。
        while active.len() < max_concurrent.load(Ordering::SeqCst) {
            let paused_folder_snapshot = folder_pause_snapshot(&paused_folders).unwrap_or_default();
            if let Some(task) = take_next_runnable_task(&mut waiting, &paused_folder_snapshot) {
                let id = task.id.clone();
                let parent_id = task.parent_id.clone();
                let (signal_tx, signal_rx) = watch::channel(TaskSignal::Running);
                signals.insert(id.clone(), signal_tx);
                active_parents.insert(id.clone(), parent_id);
                let handle = spawn_upload_task(
                    task,
                    completion_tx.clone(),
                    signal_rx,
                    app.clone(),
                    db.clone(),
                    state_sync.clone(),
                    api_resolver.clone(),
                    max_retry.clone(),
                );
                active.insert(id, handle);
            } else {
                break;
            }
        }

        tokio::select! {
            Some(task) = enqueue_rx.recv() => {
                // 新任务只入等待队列，不在这里直接执行，统一由并发填充逻辑启动。
                waiting.push_back(task);
            }
            Some(command) = control_rx.recv() => {
                match command {
                    ControlCommand::Pause { id } => {
                        // 运行中的任务通过 watch 信号暂停；尚未执行的任务直接从等待队列移除并标记 paused。
                        if let Some(signal) = signals.get(&id) {
                            let _ = safe_update_task(
                                &db,
                                id.clone(),
                                TaskUpdate {
                                    status: Some(STATUS_PAUSING.to_string()),
                                    upload_speed: Some(0),
                                    eta_secs: Some(None),
                                    ..TaskUpdate::default()
                                },
                            )
                            .await;
                            let _ = signal.send(TaskSignal::Paused);
                            let _ = upload_pause(id.clone());
                            if let Some(parent_id) = active_parents.get(&id).and_then(|parent| parent.clone()) {
                                sync_parent_folder(&db, &state_sync, &parent_id).await;
                            }
                            state_sync.notify_state_change();
                        } else if let Some(pos) = waiting.iter().position(|item| item.id == id) {
                            let parent_id = waiting[pos].parent_id.clone();
                            waiting.remove(pos);
                            let _ = safe_update_task(
                                &db,
                                id.clone(),
                                TaskUpdate {
                                    status: Some(STATUS_PAUSED.to_string()),
                                    upload_speed: Some(0),
                                    eta_secs: Some(None),
                                    ..TaskUpdate::default()
                                },
                            ).await;
                            if let Some(parent_id) = parent_id {
                                sync_parent_folder(&db, &state_sync, &parent_id).await;
                            }
                            state_sync.notify_state_change();
                        }
                    }
                    ControlCommand::Resume { task, front } => {
                        // 恢复既支持插队，也支持尾部继续排队，取决于调用场景。
                        if active.contains_key(&task.id)
                            || waiting.iter().any(|item| item.id == task.id)
                        {
                            continue;
                        }

                        if front {
                            waiting.push_front(task);
                        } else {
                            waiting.push_back(task);
                        }
                    }
                    ControlCommand::Remove { id } => {
                        // 删除任务要同时覆盖运行态、等待态和数据库残留状态。
                        if let Some(signal) = signals.get(&id) {
                            let _ = signal.send(TaskSignal::Cancelled);
                            let _ = upload_cancel(id.clone());
                        }
                        if let Some(pos) = waiting.iter().position(|item| item.id == id) {
                            waiting.remove(pos);
                        }

                        let parent_id = match db.get_task_by_id(id.clone()).await {
                            Ok(Some(task)) => task.parent_id,
                            _ => None,
                        };

                        let _ = safe_delete_task(&db, &id).await;
                        if let Some(parent_id) = parent_id {
                            sync_parent_folder(&db, &state_sync, &parent_id).await;
                        }
                        state_sync.notify_state_change();
                    }
                    ControlCommand::PauseFolder { parent_id } => {
                        // 文件夹暂停是“父任务 + 所有子任务 + 收集流程”三个层次一起暂停。
                        let mut paused_waiting = Vec::new();
                        waiting.retain(|task| {
                            if task.parent_id.as_deref() == Some(parent_id.as_str()) {
                                paused_waiting.push(task.id.clone());
                                false
                            } else {
                                true
                            }
                        });

                        for id in &paused_waiting {
                            let _ = safe_update_task(
                                &db,
                                id.clone(),
                                TaskUpdate {
                                    status: Some(STATUS_PAUSED.to_string()),
                                    upload_speed: Some(0),
                                    eta_secs: Some(None),
                                    ..TaskUpdate::default()
                                },
                            ).await;
                        }

                        let active_children: Vec<String> = active_parents
                            .iter()
                            .filter_map(|(id, parent)| {
                                (parent.as_deref() == Some(parent_id.as_str())).then_some(id.clone())
                            })
                            .collect();
                        for id in &active_children {
                            let _ = safe_update_task(
                                &db,
                                id.clone(),
                                TaskUpdate {
                                    status: Some(STATUS_PAUSING.to_string()),
                                    upload_speed: Some(0),
                                    eta_secs: Some(None),
                                    ..TaskUpdate::default()
                                },
                            )
                            .await;
                            if let Some(signal) = signals.get(id) {
                                let _ = signal.send(TaskSignal::Paused);
                            }
                            let _ = upload_pause(id.clone());
                        }

                        let folder_status = if !active_children.is_empty() || queue.is_collecting(&parent_id).unwrap_or(false) {
                            STATUS_PAUSING
                        } else {
                            STATUS_PAUSED
                        };

                        let _ = safe_update_task(
                            &db,
                            parent_id.clone(),
                            TaskUpdate {
                                status: Some(folder_status.to_string()),
                                upload_speed: Some(0),
                                eta_secs: Some(None),
                                ..TaskUpdate::default()
                            },
                        ).await;
                        sync_parent_folder(&db, &state_sync, &parent_id).await;
                        state_sync.notify_state_change();
                    }
                    ControlCommand::RemoveFolder { parent_id } => {
                        // 删除文件夹时要同时清掉等待中的子任务、运行中的子任务以及数据库记录。
                        waiting.retain(|task| task.parent_id.as_deref() != Some(parent_id.as_str()));
                        let active_children: Vec<String> = active_parents
                            .iter()
                            .filter_map(|(id, parent)| {
                                (parent.as_deref() == Some(parent_id.as_str())).then_some(id.clone())
                            })
                            .collect();
                        for id in &active_children {
                            if let Some(signal) = signals.get(id) {
                                let _ = signal.send(TaskSignal::Cancelled);
                            }
                            let _ = upload_cancel(id.clone());
                        }

                        let _ = db.delete_child_tasks(parent_id.clone()).await;
                        let _ = safe_delete_task(&db, &parent_id).await;
                        state_sync.notify_state_change();
                    }
                    ControlCommand::PauseAll { completion } => {
                        // 全部暂停除了单文件任务外，也会把当前未结束的文件夹任务一并标记为 paused。
                        pause_all_waiters.push(completion);

                        if pause_all_snapshot.is_some() {
                            continue;
                        }

                        let collecting_folder_ids = match UploadQueue::collection_lock(&collecting_folders) {
                            Ok(ids) => ids.clone(),
                            Err(error) => {
                                error!("[上传队列] 读取正在收集的文件夹失败: {}", error);
                                HashSet::new()
                            }
                        };

                        pause_all_snapshot = Some(PauseAllSnapshot {
                            active_task_ids: active.keys().cloned().collect(),
                            collecting_folder_ids: collecting_folder_ids.clone(),
                        });

                        if let Ok(mut paused) = UploadQueue::collection_lock(&paused_folders) {
                            paused.extend(collecting_folder_ids);
                        }

                        for id in signals.keys() {
                            let _ = safe_update_task(
                                &db,
                                id.clone(),
                                TaskUpdate {
                                    status: Some(STATUS_PAUSING.to_string()),
                                    upload_speed: Some(0),
                                    eta_secs: Some(None),
                                    ..TaskUpdate::default()
                                },
                            )
                            .await;
                            if let Some(signal) = signals.get(id) {
                                let _ = signal.send(TaskSignal::Paused);
                            }
                            let _ = upload_pause(id.clone());
                        }

                        for task in waiting.drain(..) {
                            let _ = safe_update_task(
                                &db,
                                task.id,
                                TaskUpdate {
                                    status: Some(STATUS_PAUSED.to_string()),
                                    upload_speed: Some(0),
                                    eta_secs: Some(None),
                                    ..TaskUpdate::default()
                                },
                            ).await;
                        }

                        let active_parent_ids: HashSet<String> = active_parents
                            .values()
                            .filter_map(|parent_id| parent_id.clone())
                            .collect();

                        if let Ok(tasks) = db.get_all_tasks().await {
                            for task in tasks.into_iter().filter(|task| {
                                task.is_folder
                                    && task.status != "complete"
                                    && task.status != "error"
                                    && task.status != "cancelled"
                            }) {
                                let next_status = if active_parent_ids.contains(&task.id)
                                    || queue.is_collecting(&task.id).unwrap_or(false)
                                {
                                    STATUS_PAUSING
                                } else {
                                    STATUS_PAUSED
                                };
                                let _ = safe_update_task(
                                    &db,
                                    task.id,
                                    TaskUpdate {
                                        status: Some(next_status.to_string()),
                                        upload_speed: Some(0),
                                        eta_secs: Some(None),
                                        ..TaskUpdate::default()
                                    },
                                ).await;
                            }
                        }
                        state_sync.notify_state_change();
                    }
                    ControlCommand::ResumeAll { completion } => {
                        let mut result = Ok(());
                        let paused_top_level_tasks = match db.get_top_level_tasks().await {
                            Ok(tasks) => tasks
                                .into_iter()
                                .filter(|task| task.status == "paused")
                                .collect::<Vec<_>>(),
                            Err(error) => {
                                error!("[上传队列] 查询暂停顶层任务失败: {}", error);
                                result = Err(error.into());
                                Vec::new()
                            }
                        };

                        for task in paused_top_level_tasks {
                            let resume_result = if task.is_folder {
                                resume_folder_impl(
                                    &app,
                                    task.id,
                                    &db,
                                    &state_sync,
                                    &queue,
                                    &api_resolver,
                                )
                                .await
                            } else {
                                resume_task_impl(task.id, &db, &state_sync, &queue).await
                            };

                            if let Err(error) = resume_result {
                                warn!("[上传队列] 全部恢复时处理任务失败: {}", error);
                                if result.is_ok() {
                                    result = Err(error);
                                }
                            }
                        }

                        pause_all_snapshot = None;
                        pause_all_requested.store(0, Ordering::SeqCst);
                        let _ = completion.send(result);
                    }
                }
            }
            Some(completion) = completion_rx.recv() => {
                // 任务完成后先回收运行态资源，再把最终状态持久化进数据库。
                let id = completion.id().to_string();
                active.remove(&id);
                signals.remove(&id);
                let parent_id = active_parents.remove(&id).flatten();

                match completion {
                    TaskCompletion::Completed { id } => {
                        let _ = safe_update_task(
                            &db,
                            id,
                            TaskUpdate {
                                status: Some("complete".to_string()),
                                progress: Some(100.0),
                                upload_speed: Some(0),
                                eta_secs: Some(None),
                                completed_at: Some(Some(now_ms())),
                                oss_upload_id: Some(None),
                                ..TaskUpdate::default()
                            },
                        ).await;
                    }
                    TaskCompletion::Failed { id, error } => {
                        let _ = safe_update_task(
                            &db,
                            id,
                            TaskUpdate {
                                status: Some("error".to_string()),
                                upload_speed: Some(0),
                                eta_secs: Some(None),
                                error_message: Some(Some(error)),
                                ..TaskUpdate::default()
                            },
                        ).await;
                    }
                    TaskCompletion::Paused { id } => {
                        let _ = safe_update_task(
                            &db,
                            id,
                            TaskUpdate {
                                status: Some(STATUS_PAUSED.to_string()),
                                upload_speed: Some(0),
                                eta_secs: Some(None),
                                ..TaskUpdate::default()
                            },
                        ).await;
                    }
                    TaskCompletion::Cancelled { id } => {
                        let _ = safe_delete_task(&db, &id).await;
                    }
                }

                if let Some(parent_id) = parent_id {
                    sync_parent_folder(&db, &state_sync, &parent_id).await;
                }
                state_sync.notify_state_change();
            }
        }

        if let Err(error) = try_finish_pause_all(
            &active,
            &collecting_folders,
            &pause_all_requested,
            &mut pause_all_waiters,
            &mut pause_all_snapshot,
        ) {
            error!("[上传队列] 检查全部暂停是否完成失败: {}", error);
        }
    }
}

fn folder_pause_snapshot(
    paused_folders: &Arc<Mutex<HashSet<String>>>,
) -> Result<HashSet<String>, UploadQueueError> {
    Ok(UploadQueue::collection_lock(paused_folders)?.clone())
}

fn is_waiting_task_blocked(task: &PendingTask, paused_folders: &HashSet<String>) -> bool {
    task.parent_id
        .as_deref()
        .is_some_and(|parent_id| paused_folders.contains(parent_id))
}

fn take_next_runnable_task(
    waiting: &mut VecDeque<PendingTask>,
    paused_folders: &HashSet<String>,
) -> Option<PendingTask> {
    let index = waiting
        .iter()
        .position(|task| !is_waiting_task_blocked(task, paused_folders))?;
    waiting.remove(index)
}

async fn pause_blocked_waiting_tasks(
    waiting: &mut VecDeque<PendingTask>,
    db: &DbHandle,
    state_sync: &UploadStateSync,
    paused_folders: &Arc<Mutex<HashSet<String>>>,
) {
    let paused_folder_snapshot = match folder_pause_snapshot(paused_folders) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            error!("[上传队列] 读取文件夹暂停状态失败: {}", error);
            return;
        }
    };

    if paused_folder_snapshot.is_empty() {
        return;
    }

    let mut blocked = Vec::new();
    waiting.retain(|task| {
        if is_waiting_task_blocked(task, &paused_folder_snapshot) {
            blocked.push((task.id.clone(), task.parent_id.clone()));
            false
        } else {
            true
        }
    });

    if blocked.is_empty() {
        return;
    }

    let mut affected_parents = HashSet::new();
    for (id, parent_id) in blocked {
        let _ = safe_update_task(
            db,
            id,
            TaskUpdate {
                status: Some(STATUS_PAUSED.to_string()),
                upload_speed: Some(0),
                eta_secs: Some(None),
                ..TaskUpdate::default()
            },
        )
        .await;

        if let Some(parent_id) = parent_id {
            affected_parents.insert(parent_id);
        }
    }

    for parent_id in affected_parents {
        sync_parent_folder(db, state_sync, &parent_id).await;
    }
    state_sync.notify_state_change();
}

fn try_finish_pause_all(
    active: &HashMap<String, JoinHandle<()>>,
    collecting_folders: &Arc<Mutex<HashSet<String>>>,
    pause_all_requested: &Arc<AtomicUsize>,
    pause_all_waiters: &mut Vec<oneshot::Sender<()>>,
    pause_all_snapshot: &mut Option<PauseAllSnapshot>,
) -> Result<(), UploadQueueError> {
    if pause_all_waiters.is_empty() {
        return Ok(());
    }

    let Some(snapshot) = pause_all_snapshot.as_ref() else {
        return Ok(());
    };

    let collecting = UploadQueue::collection_lock(collecting_folders)?;
    let has_active_targets = snapshot
        .active_task_ids
        .iter()
        .any(|task_id| active.contains_key(task_id));
    let has_collecting_targets = snapshot
        .collecting_folder_ids
        .iter()
        .any(|parent_id| collecting.contains(parent_id));
    if has_active_targets || has_collecting_targets {
        return Ok(());
    }

    pause_all_requested.store(0, Ordering::SeqCst);
    *pause_all_snapshot = None;
    for waiter in pause_all_waiters.drain(..) {
        let _ = waiter.send(());
    }

    Ok(())
}

/// 为单文件任务启动独立的后台执行 future，并把结束结果送回调度循环。
fn spawn_upload_task(
    task: PendingTask,
    completion_tx: mpsc::Sender<TaskCompletion>,
    signal_rx: watch::Receiver<TaskSignal>,
    app: AppHandle,
    db: DbHandle,
    state_sync: UploadStateSync,
    api_resolver: Arc<UploadApiResolver>,
    max_retry: Arc<AtomicUsize>,
) -> JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let completion = run_upload_task(
            task,
            signal_rx,
            app,
            db,
            state_sync,
            api_resolver,
            max_retry,
        )
        .await;
        let _ = completion_tx.send(completion).await;
    })
}

/// 单文件任务的带重试执行器。
///
/// 这里不直接关心上传细节，只负责：
/// - 调用一次真正的执行逻辑
/// - 根据错误类型决定是否继续重试
/// - 在达到上限时返回最终失败结果
async fn run_upload_task(
    task: PendingTask,
    mut signal_rx: watch::Receiver<TaskSignal>,
    app: AppHandle,
    db: DbHandle,
    state_sync: UploadStateSync,
    api_resolver: Arc<UploadApiResolver>,
    max_retry: Arc<AtomicUsize>,
) -> TaskCompletion {
    let max_attempts = max_retry.load(Ordering::SeqCst);

    for attempt in 0..=max_attempts {
        info!(
            "[上传队列] 开始尝试 id={} attempt={}/{}",
            task.id,
            attempt + 1,
            max_attempts + 1
        );
        match run_upload_task_once(&task, &mut signal_rx, &app, &db, &state_sync, &api_resolver)
            .await
        {
            Ok(TaskCompletion::Completed { .. }) => {
                info!(
                    "[上传队列] 任务完成 id={} attempt={}/{}",
                    task.id,
                    attempt + 1,
                    max_attempts + 1
                );
                return TaskCompletion::Completed {
                    id: task.id.clone(),
                };
            }
            Ok(TaskCompletion::Paused { .. }) => {
                info!(
                    "[上传队列] 任务暂停 id={} attempt={}/{}",
                    task.id,
                    attempt + 1,
                    max_attempts + 1
                );
                return TaskCompletion::Paused {
                    id: task.id.clone(),
                };
            }
            Ok(TaskCompletion::Cancelled { .. }) => {
                info!(
                    "[上传队列] 任务取消 id={} attempt={}/{}",
                    task.id,
                    attempt + 1,
                    max_attempts + 1
                );
                return TaskCompletion::Cancelled {
                    id: task.id.clone(),
                };
            }
            Ok(TaskCompletion::Failed { error, .. })
            | Err(TaskCompletion::Failed { error, .. }) => {
                if attempt >= max_attempts {
                    warn!(
                        "[上传队列] 任务失败 id={} attempt={}/{}: {}",
                        task.id,
                        attempt + 1,
                        max_attempts + 1,
                        error
                    );
                    return TaskCompletion::Failed {
                        id: task.id.clone(),
                        error,
                    };
                }
                let delay_ms = retry_delay_ms(attempt);
                warn!(
                    "[上传队列] 任务重试 id={} attempt={}/{} delay={}ms: {}",
                    task.id,
                    attempt + 1,
                    max_attempts + 1,
                    delay_ms,
                    error
                );
                if let Some(signal) =
                    wait_retry_delay_or_signal(&mut signal_rx, Duration::from_millis(delay_ms))
                        .await
                {
                    return completion_for(&task, signal);
                }
            }
            Err(TaskCompletion::Paused { .. }) => {
                info!(
                    "[上传队列] 任务暂停 id={} attempt={}/{}",
                    task.id,
                    attempt + 1,
                    max_attempts + 1
                );
                return TaskCompletion::Paused {
                    id: task.id.clone(),
                };
            }
            Err(TaskCompletion::Cancelled { .. }) => {
                info!(
                    "[上传队列] 任务取消 id={} attempt={}/{}",
                    task.id,
                    attempt + 1,
                    max_attempts + 1
                );
                return TaskCompletion::Cancelled {
                    id: task.id.clone(),
                };
            }
            Err(TaskCompletion::Completed { .. }) => {
                info!(
                    "[上传队列] 任务完成 id={} attempt={}/{}",
                    task.id,
                    attempt + 1,
                    max_attempts + 1
                );
                return TaskCompletion::Completed {
                    id: task.id.clone(),
                };
            }
        }
    }

    TaskCompletion::Failed {
        id: task.id,
        error: "上传失败：超过最大重试次数".to_string(),
    }
}

/// 单文件任务的一次完整执行尝试。
///
/// 顺序固定为：读取任务 -> 哈希准备 -> 上传计划协商 -> 执行 OSS 上传。
async fn run_upload_task_once(
    task: &PendingTask,
    signal_rx: &mut watch::Receiver<TaskSignal>,
    app: &AppHandle,
    db: &DbHandle,
    state_sync: &UploadStateSync,
    api_resolver: &Arc<UploadApiResolver>,
) -> Result<TaskCompletion, TaskCompletion> {
    if let Some(completion) = check_signal(signal_rx) {
        return Ok(completion_for(task, completion));
    }

    let Some(current_task) = get_existing_task(db, &task.id).await else {
        info!("[上传队列] 任务不存在，按取消处理 id={}", task.id);
        return Ok(TaskCompletion::Cancelled {
            id: task.id.clone(),
        });
    };

    info!(
        "[上传队列] 进入哈希阶段 id={} file={}",
        task.id, current_task.file_path
    );

    let _ = safe_update_task(
        db,
        task.id.clone(),
        TaskUpdate {
            status: Some("hashing".to_string()),
            upload_speed: Some(0),
            eta_secs: Some(None),
            error_message: Some(None),
            ..TaskUpdate::default()
        },
    )
    .await;
    state_sync.notify_state_change();

    let (sha1, pre_sha1) = if let (Some(sha1), Some(pre_sha1)) =
        (current_task.sha1.clone(), current_task.pre_sha1.clone())
    {
        info!("[上传队列] 复用已有哈希 id={}", task.id);
        (sha1, pre_sha1)
    } else {
        match compute_file_hash_internal(current_task.file_path.clone()).await {
            Ok(hash) => {
                info!("[上传队列] 哈希计算完成 id={}", task.id);
                let _ = safe_update_task(
                    db,
                    task.id.clone(),
                    TaskUpdate {
                        sha1: Some(Some(hash.sha1.clone())),
                        pre_sha1: Some(Some(hash.pre_sha1.clone())),
                        ..TaskUpdate::default()
                    },
                )
                .await;
                (hash.sha1, hash.pre_sha1)
            }
            Err(err) => {
                return Err(TaskCompletion::Failed {
                    id: task.id.clone(),
                    error: format!("计算文件哈希失败: {}", err),
                });
            }
        }
    };

    if let Some(completion) = check_signal(signal_rx) {
        return Ok(completion_for(task, completion));
    }

    let prepared = match prepare_upload_plan(
        &task.id,
        &current_task,
        &sha1,
        &pre_sha1,
        app,
        db,
        api_resolver,
    )
    .await
    {
        Ok(plan) => plan,
        Err(error) => {
            return Err(TaskCompletion::Failed {
                id: task.id.clone(),
                error,
            });
        }
    };

    if prepared.bucket.is_some() && prepared.object.is_some() && prepared.callback.is_some() {
        info!(
            "[上传队列] 上传计划已就绪 id={} mode=oss reuse_oss_upload_id={}",
            task.id,
            prepared.oss_upload_id.is_some()
        );
    } else {
        info!(
            "[上传队列] 命中秒传/直完成 id={} file_id={}",
            task.id,
            prepared.file_id.as_deref().unwrap_or("")
        );
    }

    if let Some(file_id) = prepared.file_id.clone() {
        let _ = safe_update_task(
            db,
            task.id.clone(),
            TaskUpdate {
                file_id: Some(Some(file_id)),
                ..TaskUpdate::default()
            },
        )
        .await;
    }

    if let (Some(bucket), Some(object), Some(callback)) = (
        prepared.bucket.clone(),
        prepared.object.clone(),
        prepared.callback.clone(),
    ) {
        if let Some(completion) = check_signal(signal_rx) {
            return Ok(completion_for(task, completion));
        }

        let upload_target = PreparedUploadTarget {
            bucket,
            object,
            callback,
        };
        match execute_oss_upload(
            task,
            &current_task,
            prepared.oss_upload_id.clone(),
            upload_target,
            app,
            db,
            state_sync,
            api_resolver,
        )
        .await
        {
            Ok(()) => Ok(TaskCompletion::Completed {
                id: task.id.clone(),
            }),
            Err(UploadError::Paused) => Ok(TaskCompletion::Paused {
                id: task.id.clone(),
            }),
            Err(UploadError::Cancelled) => Ok(TaskCompletion::Cancelled {
                id: task.id.clone(),
            }),
            Err(err) => Err(TaskCompletion::Failed {
                id: task.id.clone(),
                error: format!("OSS 上传失败: {}", err),
            }),
        }
    } else {
        Ok(TaskCompletion::Completed {
            id: task.id.clone(),
        })
    }
}

/// 执行真实的 OSS 上传并把事件回写数据库。
///
/// 进度事件和 OSS upload id 初始化事件都会通过 hook 回到这里，再同步进数据库，最后
/// 触发前端状态刷新。
async fn execute_oss_upload(
    pending: &PendingTask,
    task: &UploadTask,
    initial_oss_upload_id: Option<String>,
    target: PreparedUploadTarget,
    app: &AppHandle,
    db: &DbHandle,
    state_sync: &UploadStateSync,
    api_resolver: &Arc<UploadApiResolver>,
) -> Result<(), UploadError> {
    let _ = safe_update_task(
        db,
        pending.id.clone(),
        TaskUpdate {
            status: Some("uploading".to_string()),
            oss_bucket: Some(Some(target.bucket.clone())),
            oss_object: Some(Some(target.object.clone())),
            callback: Some(Some(target.callback.callback.clone())),
            callback_var: Some(Some(target.callback.callback_var.clone())),
            error_message: Some(None),
            ..TaskUpdate::default()
        },
    )
    .await;
    state_sync.notify_state_change();

    let mut current_oss_upload_id = initial_oss_upload_id;

    for token_attempt in 0..=3u8 {
        info!(
            "[上传队列] 获取上传凭证 id={} attempt={}/{}",
            pending.id,
            token_attempt + 1,
            4
        );
        let token: UploadTokenData = api_resolver
            .request(
                app,
                UploadApiRequest::Token {
                    task_id: pending.id.clone(),
                },
            )
            .await
            .map_err(|err| UploadError::Message {
                action: "获取上传凭证",
                detail: err.to_string(),
            })?;

        let token_expiration_ms = DateTime::parse_from_rfc3339(&token.expiration)
            .ok()
            .map(|dt| dt.timestamp_millis() as u64);

        if token_attempt > 0 {
            if let Some(latest_task) = get_existing_task(db, &pending.id).await {
                current_oss_upload_id = latest_task.oss_upload_id;
            }
        }

        let db_for_progress = db.clone();
        let sync_for_progress = state_sync.clone();
        let task_id_for_progress = pending.id.clone();
        let parent_id_for_progress = pending.parent_id.clone();
        let file_size_for_progress = task.file_size as u64;
        let speed_calc = Arc::new(Mutex::new(UploadSpeedCalculator::new(0.3)));
        let progress_hook = Arc::new(move |event: UploadProgressEvent| {
            let db = db_for_progress.clone();
            let sync = sync_for_progress.clone();
            let task_id = task_id_for_progress.clone();
            let parent_id = parent_id_for_progress.clone();
            let speed_calc = speed_calc.clone();
            let file_size = file_size_for_progress;
            tauri::async_runtime::spawn(async move {
                let progress = if event.total_size > 0 {
                    ((event.uploaded_size as f64 / event.total_size as f64) * 10000.0).round()
                        / 100.0
                } else {
                    0.0
                };

                let (speed, eta) = {
                    let mut calc = speed_calc.lock().unwrap();
                    let spd = calc.update(event.uploaded_size);
                    let remaining = file_size.saturating_sub(event.uploaded_size);
                    let eta = calc.eta(remaining);
                    (spd as i64, eta)
                };

                let status = if event.status == "complete" {
                    None
                } else {
                    Some("uploading".to_string())
                };

                let _ = safe_update_task(
                    &db,
                    task_id.clone(),
                    TaskUpdate {
                        progress: Some(progress),
                        uploaded_size: Some(event.uploaded_size as i64),
                        upload_speed: Some(speed),
                        eta_secs: Some(eta),
                        status,
                        ..TaskUpdate::default()
                    },
                )
                .await;
                if let Some(parent_id) = parent_id {
                    sync_parent_folder(&db, &sync, &parent_id).await;
                }
                sync.notify_state_change();
            });
        });

        let db_for_oss_init = db.clone();
        let sync_for_oss_init = state_sync.clone();
        let task_id_for_oss_init = pending.id.clone();
        let oss_init_hook = Arc::new(move |event: OssUploadInitEvent| {
            let db = db_for_oss_init.clone();
            let sync = sync_for_oss_init.clone();
            let task_id = task_id_for_oss_init.clone();
            tauri::async_runtime::spawn(async move {
                let _ = safe_update_task(
                    &db,
                    task_id,
                    TaskUpdate {
                        oss_upload_id: Some(Some(event.oss_upload_id)),
                        ..TaskUpdate::default()
                    },
                )
                .await;
                sync.notify_state_change();
            });
        });

        let hooks = UploadHooks {
            on_progress: Some(progress_hook),
            on_oss_init: Some(oss_init_hook),
        };

        match upload_to_oss_internal(
            app.clone(),
            pending.id.clone(),
            task.file_path.clone(),
            target.bucket.clone(),
            target.object.clone(),
            token.endpoint,
            token.access_key_id,
            token.access_key_secret,
            token.security_token,
            target.callback.callback.clone(),
            target.callback.callback_var.clone(),
            current_oss_upload_id.clone(),
            token_expiration_ms,
            hooks,
        )
        .await
        {
            Ok(_) => return Ok(()),
            Err(UploadError::TokenExpired) if token_attempt < 3 => {
                warn!(
                    "[上传队列] 上传凭证过期，准备重试 id={} attempt={}/{}",
                    pending.id,
                    token_attempt + 1,
                    4
                );
                continue;
            }
            Err(err) => return Err(err),
        }
    }

    error!("[上传队列] 上传凭证连续过期，终止任务 id={}", pending.id);
    Err(UploadError::TokenExpired)
}

/// 把数据库中遗留的未完成任务恢复为 paused。
async fn recover_tasks(db: &DbHandle, state_sync: &UploadStateSync) {
    let tasks = match db.get_all_tasks().await {
        Ok(tasks) => tasks,
        Err(err) => {
            error!("[上传队列] 启动恢复遗留任务失败: {}", err);
            return;
        }
    };

    let mut recovered = 0usize;

    for task in tasks {
        let should_pause = if task.is_folder {
            task.status != "complete" && task.status != "error" && task.status != "cancelled"
        } else {
            matches!(task.status.as_str(), "pending" | "hashing" | "uploading")
        };

        if should_pause {
            let _ = safe_update_task(
                db,
                task.id,
                TaskUpdate {
                    status: Some("paused".to_string()),
                    upload_speed: Some(0),
                    eta_secs: Some(None),
                    ..TaskUpdate::default()
                },
            )
            .await;
            recovered += 1;
        }
    }

    if recovered > 0 {
        info!("[上传队列] 已恢复遗留任务 count={}", recovered);
    }
    state_sync.notify_state_change();
}

/// 计算指数退避时间，并限制在 30 秒内。
fn retry_delay_ms(attempt: usize) -> u64 {
    let multiplier = 2u64.saturating_pow(attempt.min(5) as u32);
    (1000 * multiplier).min(30_000)
}

async fn wait_retry_delay_or_signal(
    signal_rx: &mut watch::Receiver<TaskSignal>,
    delay: Duration,
) -> Option<TaskSignal> {
    let timer = tokio::time::sleep(delay);
    tokio::pin!(timer);

    loop {
        tokio::select! {
            _ = &mut timer => return None,
            changed = signal_rx.changed() => {
                if changed.is_err() {
                    return None;
                }

                if let Some(signal) = check_signal(signal_rx) {
                    return Some(signal);
                }
            }
        }
    }
}

/// 当前毫秒级时间戳。
pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 读取当前任务控制信号。
fn check_signal(signal_rx: &watch::Receiver<TaskSignal>) -> Option<TaskSignal> {
    match *signal_rx.borrow() {
        TaskSignal::Running => None,
        TaskSignal::Paused => Some(TaskSignal::Paused),
        TaskSignal::Cancelled => Some(TaskSignal::Cancelled),
    }
}

/// 把控制信号转换成统一的任务结束结果。
fn completion_for(task: &PendingTask, signal: TaskSignal) -> TaskCompletion {
    match signal {
        TaskSignal::Running => TaskCompletion::Completed {
            id: task.id.clone(),
        },
        TaskSignal::Paused => TaskCompletion::Paused {
            id: task.id.clone(),
        },
        TaskSignal::Cancelled => TaskCompletion::Cancelled {
            id: task.id.clone(),
        },
    }
}

/// 从数据库安全读取最新任务快照。
pub(super) async fn get_existing_task(db: &DbHandle, id: &str) -> Option<UploadTask> {
    match db.get_task_by_id(id.to_string()).await {
        Ok(task) => task,
        Err(err) => {
            error!("[上传队列] 读取任务失败 id={}: {}", id, err);
            None
        }
    }
}

/// 更新任务时忽略“记录已不存在”的情况，方便并发收尾阶段复用。
pub(super) async fn safe_update_task(
    db: &DbHandle,
    id: String,
    updates: TaskUpdate,
) -> Result<(), UploadQueueError> {
    match db.update_task(id, updates).await {
        Ok(()) => Ok(()),
        Err(UploadStoreError::NotFound(_)) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// 删除任务时忽略“记录已不存在”的情况。
pub(super) async fn safe_delete_task(db: &DbHandle, id: &str) -> Result<(), UploadQueueError> {
    match db.delete_task(id.to_string()).await {
        Ok(()) => Ok(()),
        Err(UploadStoreError::NotFound(_)) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

async fn resume_task_impl(
    id: String,
    db: &DbHandle,
    sync: &UploadStateSync,
    queue: &UploadQueue,
) -> Result<(), UploadQueueError> {
    let task = db
        .get_task_by_id(id.clone())
        .await?
        .ok_or_else(|| UploadQueueError::NotFound(format!("task id={} not found", id)))?;

    if task.status != "paused" {
        return Ok(());
    }

    let pending = PendingTask {
        id: task.id.clone(),
        parent_id: task.parent_id.clone(),
    };
    let _ = safe_update_task(
        db,
        task.id,
        TaskUpdate {
            status: Some("pending".to_string()),
            upload_speed: Some(0),
            eta_secs: Some(None),
            error_message: Some(None),
            ..TaskUpdate::default()
        },
    )
    .await;
    if let Some(parent_id) = pending.parent_id.clone() {
        let _ = safe_update_task(
            db,
            parent_id,
            TaskUpdate {
                status: Some("uploading".to_string()),
                error_message: Some(None),
                completed_at: Some(None),
                ..TaskUpdate::default()
            },
        )
        .await;
    }
    queue.resume(pending, true).await?;
    sync.notify_state_change();
    Ok(())
}

async fn resume_folder_impl(
    app: &AppHandle,
    parent_id: String,
    db: &DbHandle,
    sync: &UploadStateSync,
    queue: &UploadQueue,
    resolver: &Arc<UploadApiResolver>,
) -> Result<(), UploadQueueError> {
    let parent = db
        .get_task_by_id(parent_id.clone())
        .await?
        .ok_or_else(|| UploadQueueError::NotFound(format!("folder id={} not found", parent_id)))?;
    if parent.status != "paused" {
        return Ok(());
    }

    let collecting = queue.is_collecting(&parent_id)?;
    queue.clear_folder_pause(&parent_id)?;
    let children = db.get_child_tasks(parent_id.clone()).await?;

    if collecting {
        let next_status = if children.is_empty() {
            "pending".to_string()
        } else {
            "uploading".to_string()
        };
        let _ = safe_update_task(
            db,
            parent_id.clone(),
            TaskUpdate {
                status: Some(next_status),
                error_message: Some(None),
                completed_at: Some(None),
                upload_speed: Some(0),
                eta_secs: Some(None),
                ..TaskUpdate::default()
            },
        )
        .await;

        for child in children
            .into_iter()
            .filter(|child| child.status == "paused")
        {
            let pending = PendingTask {
                id: child.id.clone(),
                parent_id: child.parent_id.clone(),
            };
            let _ = safe_update_task(
                db,
                child.id,
                TaskUpdate {
                    status: Some("pending".to_string()),
                    upload_speed: Some(0),
                    eta_secs: Some(None),
                    error_message: Some(None),
                    ..TaskUpdate::default()
                },
            )
            .await;
            queue.resume(pending, true).await?;
        }
        sync.notify_state_change();
        return Ok(());
    }

    if children.is_empty() {
        enqueue_folder_impl(
            app,
            db,
            sync,
            queue,
            resolver,
            parent.id,
            parent.file_path,
            parent.file_name,
            parent.target_cid,
            true,
        )
        .await?;
        return Ok(());
    }

    let _ = safe_update_task(
        db,
        parent_id.clone(),
        TaskUpdate {
            status: Some("uploading".to_string()),
            error_message: Some(None),
            completed_at: Some(None),
            ..TaskUpdate::default()
        },
    )
    .await;
    for child in children
        .into_iter()
        .filter(|child| child.status == "paused")
    {
        let pending = PendingTask {
            id: child.id.clone(),
            parent_id: child.parent_id.clone(),
        };
        let _ = safe_update_task(
            db,
            child.id,
            TaskUpdate {
                status: Some("pending".to_string()),
                upload_speed: Some(0),
                eta_secs: Some(None),
                error_message: Some(None),
                ..TaskUpdate::default()
            },
        )
        .await;
        queue.resume(pending, true).await?;
    }
    sync.notify_state_change();
    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("初始化上传队列失败")]
pub struct UploadQueueInitError;

/// 初始化上传调度器，并注册为 Tauri 全局状态。
pub fn init(app: &App) -> Result<(), UploadQueueInitError> {
    let db = app.state::<DbHandle>().inner().clone();
    let state_sync = app.state::<UploadStateSync>().inner().clone();
    let api_resolver = app.state::<Arc<UploadApiResolver>>().inner().clone();
    let queue = UploadQueue::start(app.handle().clone(), db, state_sync, api_resolver.clone());
    app.manage(queue);
    Ok(())
}

/// 动态调整上传并发上限。
#[tauri::command]
pub async fn upload_set_max_concurrent(
    n: usize,
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    queue.set_max_concurrent(n);
    Ok(())
}

/// 动态调整上传失败后的最大重试次数。
#[tauri::command]
pub async fn upload_set_max_retry(
    n: usize,
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    queue.set_max_retry(n);
    Ok(())
}

/// 批量创建普通文件上传任务并入队。
#[tauri::command]
pub async fn upload_enqueue_files(
    files: Vec<LocalUploadFileInput>,
    target_cid: String,
    db: tauri::State<'_, DbHandle>,
    sync: tauri::State<'_, UploadStateSync>,
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    for file in files {
        let id = format!("upload-{}-{}", now_ms(), Uuid::new_v4());
        db.insert_task(UploadTask {
            id: id.clone(),
            file_name: file.name,
            file_path: file.path,
            file_size: file.size,
            target_cid: target_cid.clone(),
            target_path: None,
            sha1: None,
            pre_sha1: None,
            pick_code: None,
            status: "pending".to_string(),
            progress: 0.0,
            upload_speed: 0,
            eta_secs: None,
            error_message: None,
            created_at: Some(now_ms()),
            completed_at: None,
            is_folder: false,
            parent_id: None,
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
        queue
            .enqueue(PendingTask {
                id,
                parent_id: None,
            })
            .await?;
    }
    sync.notify_state_change();
    Ok(())
}

/// 创建一个文件夹上传任务并进入目录编排流程。
#[tauri::command]
pub async fn upload_enqueue_folder(
    app: AppHandle,
    folder_path: String,
    folder_name: String,
    target_cid: String,
    db: tauri::State<'_, DbHandle>,
    sync: tauri::State<'_, UploadStateSync>,
    queue: tauri::State<'_, UploadQueue>,
    resolver: tauri::State<'_, Arc<UploadApiResolver>>,
) -> Result<(), UploadQueueError> {
    let parent_id = format!("upload-folder-{}-{}", now_ms(), Uuid::new_v4());
    enqueue_folder_impl(
        &app,
        &db,
        &sync,
        &queue,
        &resolver,
        parent_id,
        folder_path,
        folder_name,
        target_cid,
        false,
    )
    .await
}

/// 暂停单个普通上传任务。
#[tauri::command]
pub async fn upload_pause_task(
    id: String,
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    queue.pause(id).await
}

/// 恢复单个暂停任务。
#[tauri::command]
pub async fn upload_resume_task(
    id: String,
    db: tauri::State<'_, DbHandle>,
    sync: tauri::State<'_, UploadStateSync>,
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    resume_task_impl(id, &db, &sync, &queue).await
}

/// 重试单个失败任务。
#[tauri::command]
pub async fn upload_retry_task(
    id: String,
    db: tauri::State<'_, DbHandle>,
    sync: tauri::State<'_, UploadStateSync>,
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    let task = db
        .get_task_by_id(id.clone())
        .await?
        .ok_or_else(|| UploadQueueError::NotFound(format!("task id={} not found", id)))?;
    let pending = PendingTask {
        id: task.id.clone(),
        parent_id: task.parent_id.clone(),
    };
    let _ = safe_update_task(
        &db,
        task.id,
        TaskUpdate {
            status: Some("pending".to_string()),
            progress: Some(0.0),
            upload_speed: Some(0),
            eta_secs: Some(None),
            error_message: Some(None),
            uploaded_size: Some(0),
            completed_at: Some(None),
            ..TaskUpdate::default()
        },
    )
    .await;
    if let Some(parent_id) = pending.parent_id.clone() {
        let _ = safe_update_task(
            &db,
            parent_id,
            TaskUpdate {
                status: Some("uploading".to_string()),
                error_message: Some(None),
                completed_at: Some(None),
                ..TaskUpdate::default()
            },
        )
        .await;
    }
    queue.resume(pending, false).await?;
    sync.notify_state_change();
    Ok(())
}

/// 删除单个上传任务。
#[tauri::command]
pub async fn upload_remove_task(
    id: String,
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    queue.remove(id).await
}

/// 暂停整个文件夹上传，包括仍在收集目录结构的阶段。
#[tauri::command]
pub async fn upload_pause_folder(
    parent_id: String,
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    queue.pause_folder(parent_id).await
}

/// 恢复文件夹上传；如果此前尚未展开出子任务，会重新进入文件夹编排。
#[tauri::command]
pub async fn upload_resume_folder(
    app: AppHandle,
    parent_id: String,
    db: tauri::State<'_, DbHandle>,
    sync: tauri::State<'_, UploadStateSync>,
    queue: tauri::State<'_, UploadQueue>,
    resolver: tauri::State<'_, Arc<UploadApiResolver>>,
) -> Result<(), UploadQueueError> {
    resume_folder_impl(&app, parent_id, &db, &sync, &queue, &resolver).await
}

/// 全部继续。
///
/// 恢复当前主列表里所有顶层 paused 任务，并复用现有的单任务/文件夹恢复路径。
#[tauri::command]
pub async fn upload_resume_all(
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    queue.resume_all().await
}

/// 重试文件夹内失败的子任务；如果文件夹展开还没完成，会重新创建展开流程。
#[tauri::command]
pub async fn upload_retry_folder(
    app: AppHandle,
    parent_id: String,
    db: tauri::State<'_, DbHandle>,
    sync: tauri::State<'_, UploadStateSync>,
    queue: tauri::State<'_, UploadQueue>,
    resolver: tauri::State<'_, Arc<UploadApiResolver>>,
) -> Result<(), UploadQueueError> {
    queue.clear_folder_pause(&parent_id)?;
    let parent = db
        .get_task_by_id(parent_id.clone())
        .await?
        .ok_or_else(|| UploadQueueError::NotFound(format!("folder id={} not found", parent_id)))?;
    let children = db.get_child_tasks(parent_id.clone()).await?;
    if children.is_empty() {
        enqueue_folder_impl(
            &app,
            &db,
            &sync,
            &queue,
            &resolver,
            parent.id,
            parent.file_path,
            parent.file_name,
            parent.target_cid,
            true,
        )
        .await?;
        return Ok(());
    }

    let _ = safe_update_task(
        &db,
        parent_id.clone(),
        TaskUpdate {
            status: Some("uploading".to_string()),
            error_message: Some(None),
            completed_at: Some(None),
            failed_files: Some(Some(0)),
            ..TaskUpdate::default()
        },
    )
    .await;
    for child in children.into_iter().filter(|child| child.status == "error") {
        let pending = PendingTask {
            id: child.id.clone(),
            parent_id: child.parent_id.clone(),
        };
        let _ = safe_update_task(
            &db,
            child.id,
            TaskUpdate {
                status: Some("pending".to_string()),
                progress: Some(0.0),
                upload_speed: Some(0),
                eta_secs: Some(None),
                error_message: Some(None),
                uploaded_size: Some(0),
                completed_at: Some(None),
                ..TaskUpdate::default()
            },
        )
        .await;
        queue.resume(pending, false).await?;
    }
    sync.notify_state_change();
    Ok(())
}

/// 删除文件夹任务及其所有子任务。
#[tauri::command]
pub async fn upload_remove_folder(
    parent_id: String,
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    queue.cancel_collection(&parent_id)?;
    queue.clear_folder_pause(&parent_id)?;
    queue.remove_folder(parent_id).await
}

/// 暂停当前所有上传任务与文件夹收集流程。
#[tauri::command]
pub async fn upload_pause_all(
    queue: tauri::State<'_, UploadQueue>,
) -> Result<(), UploadQueueError> {
    queue.pause_all().await
}
