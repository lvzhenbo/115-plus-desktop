use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use log::{debug, error, info, warn};
use tokio::sync::{Notify, Semaphore, mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use tauri::AppHandle;

use super::events::{EventBridge, FolderAggregator, ProgressRegistry, UrlResolver};
use super::http::{ConnectionController, DownloadSignal};
use super::persistence::ProgressFile;
use super::store::{DbHandle, DmError, DownloadTask as StoreDownloadTask, TaskUpdate};
use super::types::{DownloadConfig, DownloadError, TaskAbortReason};

const ERR_QUEUE_CHANNEL_CLOSED: &str = "下载队列不可用：调度通道已关闭";
const ERR_PAUSE_ALL_REPLY_DROPPED: &str = "下载队列不可用：暂停确认通道已断开";

// ==================== 类型定义 ====================

/// 文件夹中的单个文件项。
///
/// 由前端收集后传给 download_enqueue_folder，用于批量创建子下载任务。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderFileItem {
    pub fid: String,
    pub name: String,
    pub pick_code: String,
    pub size: i64,
    /// 相对路径（文件夹内的子路径）
    pub path: String,
}

/// 入队请求。
///
/// 前端命令层和恢复逻辑都会构造该对象，交给主循环统一调度。
pub struct EnqueueRequest {
    pub gid: String,
    #[allow(dead_code)] // 恢复或重试时需要保留，spawn_download_task 本身不直接读取。
    pub fid: String,
    pub name: String,
    pub pick_code: String,
    pub size: i64,
    pub save_path: String,
    pub expected_sha1: Option<String>,
    pub parent_gid: Option<String>,
    pub token: String,
    pub user_agent: String,
    pub split: u16,
    pub max_global_connections: u16,
}

/// 生命周期控制指令。
///
/// 由 Tauri 命令层发送到 queue_loop，统一处理暂停、取消、恢复和重试。
pub enum ControlCommand {
    /// 暂停活跃或等待中的任务 (per CTL-01)
    Pause {
        gid: String,
    },
    /// 取消活跃或等待中的任务 (per CTL-03)
    Cancel {
        gid: String,
    },
    /// 恢复暂停的任务 — 插入队列头部
    Resume(EnqueueRequest),
    /// 重试失败的任务 — 插入队列尾部
    Retry(EnqueueRequest),
    // 文件夹级联控制操作。
    PauseFolder {
        parent_gid: String,
    },
    CancelFolder {
        parent_gid: String,
    },
    ResumeFolderChildren(Vec<EnqueueRequest>),
    RetryFolderChildren(Vec<EnqueueRequest>),
    /// 全部暂停 — 冻结队列 + 暂停所有活跃任务 (per CTL-05, D-01, D-03)
    PauseAll {
        completion: oneshot::Sender<()>,
    },
    /// 全部继续 — 解冻队列 + 恢复所有暂停任务 (per CTL-06, D-06)
    ResumeAll {
        token: String,
        user_agent: String,
        split: u16,
        max_global_connections: u16,
    },
}

/// 任务完成回报。
///
/// 子任务通过 completion_tx 上报主循环，明确区分完成、失败、暂停和取消。
pub enum TaskCompletion {
    /// 下载成功完成
    Completed { gid: String },
    /// 下载失败（非用户操作）
    Failed { gid: String, error: String },
    /// SHA1 校验失败
    VerifyFailed { gid: String, message: String },
    /// 用户暂停 — 释放槽位，保留 DB 记录
    Paused { gid: String },
    /// 用户取消 — 释放槽位，删除 DB 记录
    Cancelled { gid: String },
}

impl TaskCompletion {
    pub fn gid(&self) -> &str {
        match self {
            Self::Completed { gid }
            | Self::Failed { gid, .. }
            | Self::VerifyFailed { gid, .. }
            | Self::Paused { gid }
            | Self::Cancelled { gid } => gid,
        }
    }
}

/// 一次“全部暂停”在点击瞬间锁定的活跃任务快照。
///
/// 只等待这批已经在运行的下载任务真正停下来；暂停期间新进入 waiting 的任务不属于本次
/// 快照，不应拖慢更新安装或退出关闭。
struct PauseAllSnapshot {
    active_gids: HashSet<String>,
}

// ==================== 队列调度器 ====================

/// Rust 端并发下载队列调度器。
///
/// 通过 `app.manage()` 注册为 Tauri 全局 state，
/// Tauri commands 通过 `State<TaskQueue>` 访问。
pub struct TaskQueue {
    enqueue_tx: mpsc::Sender<EnqueueRequest>,
    control_tx: mpsc::Sender<ControlCommand>,
    max_concurrent: Arc<AtomicUsize>,
    wake_notify: Arc<Notify>,
    folder_aggregator: Arc<FolderAggregator>,
    #[allow(dead_code)] // 仅供 PauseAll/ResumeAll 控制流使用，对外不暴露读取。
    frozen: Arc<AtomicBool>,
}

impl TaskQueue {
    /// 创建 TaskQueue 并启动主调度循环。
    pub fn start(
        app: AppHandle,
        db: DbHandle,
        state_sync_notify: Arc<Notify>,
        url_resolver: Arc<UrlResolver>,
        progress_registry: Arc<ProgressRegistry>,
        http_client: reqwest::Client,
        progress_file: Arc<ProgressFile>,
        folder_aggregator: Arc<FolderAggregator>,
    ) -> Self {
        let (enqueue_tx, enqueue_rx) = mpsc::channel::<EnqueueRequest>(256);
        let (completion_tx, completion_rx) = mpsc::channel::<TaskCompletion>(256);
        let (control_tx, control_rx) = mpsc::channel::<ControlCommand>(64);
        let max_concurrent = Arc::new(AtomicUsize::new(3));
        let wake_notify = Arc::new(Notify::new());
        let frozen = Arc::new(AtomicBool::new(false));

        tauri::async_runtime::spawn(queue_loop(
            enqueue_rx,
            completion_tx,
            completion_rx,
            control_rx,
            max_concurrent.clone(),
            wake_notify.clone(),
            app,
            db,
            state_sync_notify,
            url_resolver,
            progress_registry,
            http_client,
            progress_file,
            folder_aggregator.clone(),
            frozen.clone(),
        ));

        Self {
            enqueue_tx,
            control_tx,
            max_concurrent,
            wake_notify,
            folder_aggregator,
            frozen,
        }
    }

    /// 将任务发送到主循环对应通道，等待调度。
    pub async fn enqueue(&self, req: EnqueueRequest) -> Result<(), DmError> {
        self.enqueue_tx
            .send(req)
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    /// 修改最大并发数并唤醒主循环重新分配槽位。
    pub fn set_max_concurrent(&self, n: usize) {
        let clamped = n.clamp(1, 5);
        self.max_concurrent.store(clamped, Ordering::SeqCst);
        self.wake_notify.notify_one();
    }

    /// 请求暂停指定任务。
    pub async fn pause(&self, gid: String) -> Result<(), DmError> {
        self.control_tx
            .send(ControlCommand::Pause { gid })
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    /// 请求取消指定任务。
    pub async fn cancel(&self, gid: String) -> Result<(), DmError> {
        self.control_tx
            .send(ControlCommand::Cancel { gid })
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    /// 恢复已暂停任务，并插入队列头部优先调度。
    pub async fn resume(&self, req: EnqueueRequest) -> Result<(), DmError> {
        self.control_tx
            .send(ControlCommand::Resume(req))
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    /// 重试失败任务，并插入队列尾部保持 FIFO。
    pub async fn retry(&self, req: EnqueueRequest) -> Result<(), DmError> {
        self.control_tx
            .send(ControlCommand::Retry(req))
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    pub async fn pause_folder(&self, parent_gid: String) -> Result<(), DmError> {
        self.control_tx
            .send(ControlCommand::PauseFolder { parent_gid })
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    pub async fn cancel_folder(&self, parent_gid: String) -> Result<(), DmError> {
        self.control_tx
            .send(ControlCommand::CancelFolder { parent_gid })
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    pub async fn resume_folder_children(
        &self,
        children: Vec<EnqueueRequest>,
    ) -> Result<(), DmError> {
        self.control_tx
            .send(ControlCommand::ResumeFolderChildren(children))
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    pub async fn retry_folder_children(
        &self,
        children: Vec<EnqueueRequest>,
    ) -> Result<(), DmError> {
        self.control_tx
            .send(ControlCommand::RetryFolderChildren(children))
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }

    /// 全部暂停 — 冻结队列 + 暂停所有活跃任务 (per CTL-05, D-05)
    pub async fn pause_all(&self) -> Result<(), DmError> {
        let (tx, rx) = oneshot::channel();
        self.control_tx
            .send(ControlCommand::PauseAll { completion: tx })
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))?;

        rx.await
            .map_err(|_| DmError::Internal(ERR_PAUSE_ALL_REPLY_DROPPED.into()))
    }

    /// 全部继续 — 解冻队列 + 恢复所有暂停任务 (per CTL-06, D-05)
    pub async fn resume_all(
        &self,
        token: String,
        user_agent: String,
        split: u16,
        max_global_connections: u16,
    ) -> Result<(), DmError> {
        self.control_tx
            .send(ControlCommand::ResumeAll {
                token,
                user_agent,
                split,
                max_global_connections,
            })
            .await
            .map_err(|_| DmError::Internal(ERR_QUEUE_CHANNEL_CLOSED.into()))
    }
}

// ==================== 主循环 ====================

/// 队列主循环。
///
/// 负责管理等待队列、活跃任务、控制指令以及完成回报。
async fn queue_loop(
    mut enqueue_rx: mpsc::Receiver<EnqueueRequest>,
    completion_tx: mpsc::Sender<TaskCompletion>,
    mut completion_rx: mpsc::Receiver<TaskCompletion>,
    mut control_rx: mpsc::Receiver<ControlCommand>,
    max_concurrent: Arc<AtomicUsize>,
    wake_notify: Arc<Notify>,
    app: AppHandle,
    db: DbHandle,
    state_sync_notify: Arc<Notify>,
    url_resolver: Arc<UrlResolver>,
    progress_registry: Arc<ProgressRegistry>,
    http_client: reqwest::Client,
    progress_file: Arc<ProgressFile>,
    folder_aggregator: Arc<FolderAggregator>,
    frozen: Arc<AtomicBool>,
) {
    let mut waiting: VecDeque<EnqueueRequest> = VecDeque::new();
    let mut active: HashMap<String, JoinHandle<()>> = HashMap::new();
    let mut signals: HashMap<String, watch::Sender<DownloadSignal>> = HashMap::new();
    let mut child_to_parent: HashMap<String, String> = HashMap::new();
    let mut pause_all_waiters: Vec<oneshot::Sender<()>> = Vec::new();
    let mut pause_all_snapshot: Option<PauseAllSnapshot> = None;

    // 分片级并发控制器，替代旧的全局下载信号量。
    let mut current_segment_limit: usize = 0;
    let mut segment_semaphore: Arc<Semaphore> = Arc::new(Semaphore::new(0));
    let mut conn_controller: Arc<ConnectionController> = Arc::new(ConnectionController::new(0));

    // 启动恢复必须先完成，之后主循环才开始接收新请求。
    recover_tasks(&db, &progress_file, &folder_aggregator, &state_sync_notify).await;

    loop {
        // 尝试填补空位 — 出队 waiting 任务并 spawn 下载
        while active.len() < max_concurrent.load(Ordering::SeqCst) && !frozen.load(Ordering::SeqCst)
        {
            if let Some(req) = waiting.pop_front() {
                let gid = req.gid.clone();

                // 如果任务级全局连接数变化，重建分片并发控制器。
                let new_limit = req.max_global_connections as usize;
                if new_limit != current_segment_limit {
                    segment_semaphore = Arc::new(Semaphore::new(new_limit));
                    conn_controller = Arc::new(ConnectionController::new(new_limit as u16));
                    current_segment_limit = new_limit;
                }

                // 记录子任务到父文件夹的映射，供文件夹完成态判断使用。
                if let Some(ref parent) = req.parent_gid {
                    child_to_parent.insert(gid.clone(), parent.clone());
                }

                // 创建信号通道，sender 留在 signals 注册表，receiver 传入 spawn
                let (signal_tx, signal_rx) = watch::channel(DownloadSignal::Running);
                signals.insert(gid.clone(), signal_tx);

                let handle = spawn_download_task(
                    req,
                    completion_tx.clone(),
                    signal_rx,
                    app.clone(),
                    db.clone(),
                    state_sync_notify.clone(),
                    url_resolver.clone(),
                    progress_registry.clone(),
                    http_client.clone(),
                    progress_file.clone(),
                    segment_semaphore.clone(),
                    conn_controller.clone(),
                );
                active.insert(gid, handle);
            } else {
                break;
            }
        }

        // 同时监听入队、控制、完成回报和唤醒通知四类事件。
        tokio::select! {
            Some(req) = enqueue_rx.recv() => {
                debug!("[队列] 入队 gid={} name={}", req.gid, req.name);
                waiting.push_back(req);
            }
            Some(cmd) = control_rx.recv() => {
                match cmd {
                    ControlCommand::Pause { gid } => {
                        if let Some(tx) = signals.get(&gid) {
                            // 通过内部信号注册表发送暂停，避免依赖全局静态表。
                            let _ = tx.send(DownloadSignal::Paused);
                            // active 清理和数据库更新由 completion_rx 的 Paused 分支统一处理。
                        } else if let Some(pos) = waiting.iter().position(|r| r.gid == gid) {
                            // 等待队列中的任务：直接移除 + 更新 DB
                            waiting.remove(pos);
                            if let Err(e) = db
                                .update_task(
                                    gid.clone(),
                                    TaskUpdate {
                                        status: Some("paused".to_string()),
                                        download_speed: Some(0),
                                        eta: Some(None),
                                        ..TaskUpdate::default()
                                    },
                                )
                                .await
                            {
                                error!("[队列] 暂停等待中任务失败 {}: {}", gid, e);
                            }
                            state_sync_notify.notify_one();
                        } else {
                            warn!("[队列] 暂停失败，未找到任务 gid={}", gid);
                        }
                    }
                    ControlCommand::Cancel { gid } => {
                        if let Some(tx) = signals.get(&gid) {
                            let _ = tx.send(DownloadSignal::Cancelled);
                        } else if let Some(pos) = waiting.iter().position(|r| r.gid == gid) {
                            waiting.remove(pos);
                            if let Err(e) = db.delete_task(gid.clone()).await {
                                error!("[队列] 删除取消的等待中任务失败 {}: {}", gid, e);
                            }
                            state_sync_notify.notify_one();
                        } else {
                            match db.delete_task(gid.clone()).await {
                                Ok(()) => {
                                    progress_registry.remove(&gid);
                                    state_sync_notify.notify_one();
                                    info!("[队列] 已删除非活跃任务 gid={}", gid);
                                }
                                Err(DmError::NotFound(_)) => {
                                    warn!("[队列] 取消失败，未找到任务 gid={}", gid);
                                }
                                Err(e) => {
                                    error!("[队列] 删除非活跃任务失败 {}: {}", gid, e);
                                }
                            }
                        }
                    }
                    ControlCommand::Resume(req) => {
                        debug!("[队列] 恢复 gid={}", req.gid);
                        waiting.push_front(req);
                    }
                    ControlCommand::Retry(req) => {
                        debug!("[队列] 重试 gid={}", req.gid);
                        waiting.push_back(req);
                    }
                    ControlCommand::PauseFolder { parent_gid } => {
                        info!("[队列] 暂停文件夹 gid={}", parent_gid);

                        // 1. 从等待队列移除该文件夹下尚未启动的子任务。
                        let mut paused_waiting_gids = Vec::new();
                        waiting.retain(|req| {
                            if req.parent_gid.as_deref() == Some(parent_gid.as_str()) {
                                paused_waiting_gids.push(req.gid.clone());
                                false
                            } else {
                                true
                            }
                        });

                        // 2. 将刚移除的等待中子任务状态写回数据库为 paused。
                        for gid in &paused_waiting_gids {
                            if let Err(e) = db
                                .update_task(
                                    gid.clone(),
                                    TaskUpdate {
                                        status: Some("paused".to_string()),
                                        download_speed: Some(0),
                                        eta: Some(None),
                                        ..TaskUpdate::default()
                                    },
                                )
                                .await
                            {
                                error!("[队列] 暂停等待中子任务失败 {}: {}", gid, e);
                            }
                        }

                        // 3. 给仍在运行的子任务发送暂停信号。
                        let active_children: Vec<String> = child_to_parent
                            .iter()
                            .filter(|(_, p)| *p == &parent_gid)
                            .map(|(gid, _)| gid.clone())
                            .collect();
                        for gid in &active_children {
                            if let Some(tx) = signals.get(gid) {
                                let _ = tx.send(DownloadSignal::Paused);
                            }
                        }

                        // 4. 将父文件夹任务也标记为 paused。
                        if let Err(e) = db
                            .update_task(
                                parent_gid.clone(),
                                TaskUpdate {
                                    status: Some("paused".to_string()),
                                    download_speed: Some(0),
                                    eta: Some(None),
                                    ..TaskUpdate::default()
                                },
                            )
                            .await
                        {
                            error!("[队列] 暂停文件夹失败 {}: {}", parent_gid, e);
                        }

                        persist_folder_progress(&db, &folder_aggregator, &parent_gid).await;

                        state_sync_notify.notify_one();
                        info!(
                            "[队列] 文件夹{}已暂停: {}个等待中, {}个活跃中",
                            parent_gid, paused_waiting_gids.len(), active_children.len()
                        );
                    }
                    ControlCommand::CancelFolder { parent_gid } => {
                        info!("[队列] 取消文件夹 gid={}", parent_gid);

                        // 1. 从等待队列直接移除该文件夹下未启动的子任务。
                        waiting.retain(|req| {
                            req.parent_gid.as_deref() != Some(parent_gid.as_str())
                        });

                        // 2. 给仍在运行的子任务发送取消信号。
                        let active_children: Vec<String> = child_to_parent
                            .iter()
                            .filter(|(_, p)| *p == &parent_gid)
                            .map(|(gid, _)| gid.clone())
                            .collect();
                        for gid in &active_children {
                            if let Some(tx) = signals.get(gid) {
                                let _ = tx.send(DownloadSignal::Cancelled);
                            }
                        }

                        // 3. 删除数据库中的子任务和父文件夹任务记录。
                        if let Err(e) = db.delete_child_tasks(parent_gid.clone()).await {
                            error!("[队列] 删除文件夹子任务失败 {}: {}", parent_gid, e);
                        }
                        if let Err(e) = db.delete_task(parent_gid.clone()).await {
                            error!("[队列] 删除文件夹失败 {}: {}", parent_gid, e);
                        }

                        // 4. 清理内存中的父子映射与聚合状态。
                        child_to_parent.retain(|_, p| p != &parent_gid);
                        folder_aggregator.remove_folder(&parent_gid);
                        for gid in &active_children {
                            progress_registry.remove(gid);
                        }

                        state_sync_notify.notify_one();
                        info!(
                            "[队列] 文件夹{}已取消: {}个活跃已发信号",
                            parent_gid, active_children.len()
                        );
                    }
                    ControlCommand::ResumeFolderChildren(children) => {
                        info!("[队列] 恢复文件夹子任务: {}个任务", children.len());
                        for req in children.into_iter().rev() {
                            if let Some(ref parent) = req.parent_gid {
                                child_to_parent.insert(req.gid.clone(), parent.clone());
                            }
                            waiting.push_front(req);
                        }
                    }
                    ControlCommand::RetryFolderChildren(children) => {
                        info!("[队列] 重试文件夹子任务: {}个任务", children.len());
                        for req in children {
                            if let Some(ref parent) = req.parent_gid {
                                child_to_parent.insert(req.gid.clone(), parent.clone());
                            }
                            waiting.push_back(req);
                        }
                    }
                    ControlCommand::PauseAll { completion } => {
                        info!("[队列] 全部暂停");
                        frozen.store(true, Ordering::SeqCst);

                        if pause_all_snapshot.is_some() {
                            pause_all_waiters.push(completion);
                            continue;
                        }

                        let active_count = active.len();
                        if active_count > 0 {
                            pause_all_snapshot = Some(PauseAllSnapshot {
                                active_gids: active.keys().cloned().collect(),
                            });
                            pause_all_waiters.push(completion);
                        } else {
                            let _ = completion.send(());
                        }

                        // 遍历所有活跃任务发送暂停信号
                        for (_gid, tx) in signals.iter() {
                            let _ = tx.send(DownloadSignal::Paused);
                        }

                        // 将等待中的任务迁出内存队列并写回 paused，避免 resume_all 之后重复入队。
                        // frozen 只是内存态标记，若不持久化会影响崩溃恢复判断。
                        let paused_waiting: Vec<EnqueueRequest> = waiting.drain(..).collect();
                        for req in &paused_waiting {
                            if let Err(e) = db.update_task(
                                req.gid.clone(),
                                TaskUpdate {
                                    status: Some("paused".to_string()),
                                    download_speed: Some(0),
                                    eta: Some(None),
                                    ..TaskUpdate::default()
                                },
                            ).await {
                                error!("[队列] 暂停等待中任务失败 {}: {}", req.gid, e);
                            }
                        }

                        // 收集所有需要同步为 paused 的文件夹父任务 gid。
                        let mut parent_gids: std::collections::HashSet<String> = std::collections::HashSet::new();
                        // 先收集活跃子任务对应的父任务。
                        for parent_gid in child_to_parent.values() {
                            parent_gids.insert(parent_gid.clone());
                        }
                        // child_to_parent 只覆盖活跃任务，刚暂停的等待中子任务要额外扫描。
                        for req in &paused_waiting {
                            if let Some(ref parent) = req.parent_gid {
                                parent_gids.insert(parent.clone());
                            }
                        }

                        // 将涉及到的父文件夹任务同步为 paused。
                        for parent_gid in &parent_gids {
                            if let Err(e) = db.update_task(
                                parent_gid.clone(),
                                TaskUpdate {
                                    status: Some("paused".to_string()),
                                    download_speed: Some(0),
                                    eta: Some(None),
                                    ..TaskUpdate::default()
                                },
                            ).await {
                                error!("[队列] 暂停文件夹失败 {}: {}", parent_gid, e);
                            }
                            persist_folder_progress(&db, &folder_aggregator, parent_gid).await;
                        }

                        state_sync_notify.notify_one();
                        info!(
                            "[队列] 全部暂停完成: {}个活跃已发信号, {}个等待已暂停, {}个文件夹已暂停",
                            active_count, paused_waiting.len(), parent_gids.len()
                        );
                    }
                    ControlCommand::ResumeAll { token, user_agent, split, max_global_connections } => {
                        info!("[队列] 全部恢复");
                        frozen.store(false, Ordering::SeqCst);

                        // 查询所有处于 paused 的顶层任务，统一恢复。
                        let paused_tasks = match db.get_paused_top_level_tasks().await {
                            Ok(tasks) => tasks,
                            Err(e) => {
                                error!("[队列] 查询暂停任务失败: {}", e);
                                wake_notify.notify_one();
                                continue;
                            }
                        };

                        let mut resume_requests: Vec<EnqueueRequest> = Vec::new();

                        for task in paused_tasks {
                            if task.is_folder {
                                // 文件夹任务：查询 paused 子任务并批量恢复入队。
                                let paused_children = db
                                    .get_child_tasks_by_status(task.gid.clone(), "paused".to_string())
                                    .await
                                    .unwrap_or_default();

                                for child in &paused_children {
                                    if let Err(e) = db.update_task(child.gid.clone(), TaskUpdate {
                                        status: Some("waiting".to_string()),
                                        ..TaskUpdate::default()
                                    }).await {
                                        error!("[队列] 重置子任务失败 {}: {}", child.gid, e);
                                    }
                                }

                                // 父文件夹任务先切回 active，便于前端感知恢复态。
                                let _ = db.update_task(task.gid.clone(), TaskUpdate {
                                    status: Some("active".to_string()),
                                    ..TaskUpdate::default()
                                }).await;

                                if let Err(e) = hydrate_folder_aggregator_state(
                                    &db,
                                    progress_file.as_ref(),
                                    &folder_aggregator,
                                    &task,
                                ).await {
                                    error!("[队列] 恢复文件夹聚合状态失败 {}: {}", task.gid, e);
                                }
                                for child in paused_children {
                                    child_to_parent.insert(child.gid.clone(), task.gid.clone());
                                    resume_requests.push(EnqueueRequest {
                                        gid: child.gid,
                                        fid: child.fid,
                                        name: child.name,
                                        pick_code: child.pick_code,
                                        size: child.size,
                                        save_path: child.path.unwrap_or_default(),
                                        expected_sha1: None,
                                        parent_gid: child.parent_gid,
                                        token: token.clone(),
                                        user_agent: user_agent.clone(),
                                        split,
                                        max_global_connections,
                                    });
                                }
                            } else {
                                // 单文件任务直接重建为 EnqueueRequest。
                                let _ = db.update_task(task.gid.clone(), TaskUpdate {
                                    status: Some("waiting".to_string()),
                                    ..TaskUpdate::default()
                                }).await;
                                resume_requests.push(EnqueueRequest {
                                    gid: task.gid,
                                    fid: task.fid,
                                    name: task.name,
                                    pick_code: task.pick_code,
                                    size: task.size,
                                    save_path: task.path.unwrap_or_default(),
                                    expected_sha1: None,
                                    parent_gid: None,
                                    token: token.clone(),
                                    user_agent: user_agent.clone(),
                                    split,
                                    max_global_connections,
                                });
                            }
                        }

                        // 倒序 push_front，保证恢复后仍尽量保持原始顺序。
                        for req in resume_requests.into_iter().rev() {
                            waiting.push_front(req);
                        }

                        state_sync_notify.notify_one();
                        wake_notify.notify_one();
                        info!("[队列] 全部恢复完成");
                    }
                }
            }
            Some(completion) = completion_rx.recv() => {
                let gid = completion.gid().to_string();
                active.remove(&gid);
                signals.remove(&gid);

                match completion {
                    TaskCompletion::Completed { ref gid } => {
                        info!("[队列] 任务完成 gid={}", gid);
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as i64;
                        if let Err(e) = db
                            .update_task(
                                gid.clone(),
                                TaskUpdate {
                                    status: Some("complete".to_string()),
                                    completed_at: Some(Some(now_ms)),
                                    progress: Some(100.0),
                                    download_speed: Some(0),
                                    eta: Some(None),
                                    ..TaskUpdate::default()
                                },
                            )
                            .await
                        {
                            error!("[队列] 更新已完成任务失败 {}: {}", gid, e);
                        }
                    }
                    TaskCompletion::Failed { ref gid, ref error } => {
                        warn!("[队列] 任务失败 gid={}: {}", gid, error);
                        if let Err(e) = db
                            .update_task(
                                gid.clone(),
                                TaskUpdate {
                                    status: Some("error".to_string()),
                                    error_message: Some(Some(error.clone())),
                                    download_speed: Some(0),
                                    eta: Some(None),
                                    ..TaskUpdate::default()
                                },
                            )
                            .await
                        {
                            error!("[队列] 更新失败任务失败 {}: {}", gid, e);
                        }
                    }
                    TaskCompletion::VerifyFailed {
                        ref gid,
                        ref message,
                    } => {
                        warn!("[队列] 任务校验失败 gid={}: {}", gid, message);
                        if let Err(e) = db
                            .update_task(
                                gid.clone(),
                                TaskUpdate {
                                    status: Some("verify_failed".to_string()),
                                    error_message: Some(Some(message.clone())),
                                    download_speed: Some(0),
                                    eta: Some(None),
                                    ..TaskUpdate::default()
                                },
                            )
                            .await
                        {
                            error!("[队列] 更新校验失败任务失败 {}: {}", gid, e);
                        }
                    }
                    TaskCompletion::Paused { ref gid } => {
                        info!("[队列] 任务暂停 gid={}", gid);
                        let paused_progress = match progress_file.get_task_progress(gid) {
                            Ok(Some(snapshot)) => {
                                debug!(
                                    "[队列] 暂停任务进度 gid={} downloaded={}/{} ({:.2}%)",
                                    gid,
                                    snapshot.downloaded_bytes,
                                    snapshot.total_bytes,
                                    snapshot.progress,
                                );
                                Some(snapshot.progress)
                            }
                            Ok(None) => None,
                            Err(e) => {
                                warn!("[队列] 读取暂停任务进度失败 {}: {}", gid, e);
                                None
                            }
                        };
                        if let Err(e) = db
                            .update_task(
                                gid.clone(),
                                TaskUpdate {
                                    status: Some("paused".to_string()),
                                    progress: paused_progress,
                                    download_speed: Some(0),
                                    eta: Some(None),
                                    ..TaskUpdate::default()
                                },
                            )
                            .await
                        {
                            error!("[队列] 更新暂停任务失败 {}: {}", gid, e);
                        }
                    }
                    TaskCompletion::Cancelled { ref gid } => {
                        info!("[队列] 任务取消 gid={} (DB记录已删除, 本地文件与.oofp保留)", gid);
                        if let Err(e) = db.delete_task(gid.clone()).await {
                            error!("[队列] 删除取消任务失败 {}: {}", gid, e);
                        }
                    }
                }

                // 即时推送单任务状态变更，弥补 state-sync 150ms 去抖延迟
                if !matches!(completion, TaskCompletion::Cancelled { .. }) {
                    super::events::emit_download_task_status(&app, &db, &gid).await;
                }

                // Phase 5: Folder counter update + final status determination
                if let Some(parent_gid) = child_to_parent.remove(&gid) {
                    let child_task = db.get_task_by_gid(gid.clone()).await.ok().flatten();

                    match &completion {
                        TaskCompletion::Completed { .. } => {
                            if let Err(e) = db
                                .increment_folder_counter(parent_gid.clone(), "completed_files".to_string(), 1)
                                .await
                            {
                                error!("[队列] 递增文件夹完成数失败 {}: {}", parent_gid, e);
                            }
                            let child_size = child_task.as_ref().map(task_total_bytes).unwrap_or(0);
                            folder_aggregator.increment_completed(&gid, child_size);
                        }
                        TaskCompletion::Failed { .. } | TaskCompletion::VerifyFailed { .. } => {
                            if let Err(e) = db
                                .increment_folder_counter(parent_gid.clone(), "failed_files".to_string(), 1)
                                .await
                            {
                                error!("[队列] 递增文件夹失败数失败 {}: {}", parent_gid, e);
                            }
                            if let Some(task) = child_task.as_ref() {
                                folder_aggregator.update_child_progress(
                                    &gid,
                                    estimate_task_downloaded_bytes(progress_file.as_ref(), task),
                                );
                            }
                            folder_aggregator.increment_failed(&gid);
                        }
                        TaskCompletion::Paused { .. } => {
                            if let Some(task) = child_task.as_ref() {
                                folder_aggregator.update_child_progress(
                                    &gid,
                                    estimate_task_downloaded_bytes(progress_file.as_ref(), task),
                                );
                            }
                        }
                        TaskCompletion::Cancelled { .. } => {}
                    }

                    if matches!(completion, TaskCompletion::Completed { .. } | TaskCompletion::Cancelled { .. }) {
                        folder_aggregator.remove_child(&gid);
                    }

                    persist_folder_progress(&db, &folder_aggregator, &parent_gid).await;

                    if matches!(completion, TaskCompletion::Completed { .. } | TaskCompletion::Failed { .. } | TaskCompletion::VerifyFailed { .. }) {
                        let has_active_children = child_to_parent.values().any(|p| p == &parent_gid);
                        let has_waiting_children = waiting.iter().any(|r| {
                            r.parent_gid.as_deref() == Some(&parent_gid)
                        });

                        if !has_active_children && !has_waiting_children {
                            if let Ok(Some(parent)) = db.get_task_by_gid(parent_gid.clone()).await {
                                if parent.status != "paused" {
                                    let completed = parent.completed_files.unwrap_or(0);
                                    let failed = parent.failed_files.unwrap_or(0);
                                    let total = parent.total_files.unwrap_or(0);

                                    if let Some(final_status) = determine_folder_final_status(completed, failed, total) {
                                        info!(
                                            "[队列] 文件夹{}最终状态: {} (成功={}, 失败={}, 总计={})",
                                            parent_gid, final_status, completed, failed, total
                                        );
                                        let now_ms = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis() as i64;
                                        if let Err(e) = db
                                            .update_task(
                                                parent_gid.clone(),
                                                TaskUpdate {
                                                    status: Some(final_status.to_string()),
                                                    progress: Some(100.0),
                                                    download_speed: Some(0),
                                                    eta: Some(None),
                                                    completed_at: Some(Some(now_ms)),
                                                    ..TaskUpdate::default()
                                                },
                                            )
                                            .await
                                        {
                                            error!("[队列] 设置文件夹{}最终状态失败: {}", parent_gid, e);
                                        }
                                        folder_aggregator.remove_folder(&parent_gid);
                                    }
                                }
                            }
                        }
                    }
                }

                progress_registry.remove(&gid);
                state_sync_notify.notify_one();
            }
            _ = wake_notify.notified() => {
                // 被 set_max_concurrent 唤醒，下一轮循环检查出队
            }
        }

        try_finish_pause_all(&active, &mut pause_all_waiters, &mut pause_all_snapshot);
    }
}

fn try_finish_pause_all(
    active: &HashMap<String, JoinHandle<()>>,
    waiters: &mut Vec<oneshot::Sender<()>>,
    snapshot: &mut Option<PauseAllSnapshot>,
) {
    if waiters.is_empty() {
        return;
    }

    let Some(current_snapshot) = snapshot.as_ref() else {
        return;
    };

    if current_snapshot
        .active_gids
        .iter()
        .any(|gid| active.contains_key(gid))
    {
        return;
    }

    *snapshot = None;
    for waiter in waiters.drain(..) {
        let _ = waiter.send(());
    }
}

// ==================== Helpers ====================

/// 根据子任务完成情况推导文件夹最终状态。
fn determine_folder_final_status(
    completed_files: i64,
    failed_files: i64,
    total_files: i64,
) -> Option<&'static str> {
    let finished = completed_files + failed_files;
    if finished < total_files {
        return None;
    }
    if failed_files == 0 {
        Some("complete")
    } else if completed_files > 0 {
        Some("partial_error")
    } else {
        Some("error")
    }
}

fn task_total_bytes(task: &StoreDownloadTask) -> u64 {
    task.size.max(0) as u64
}

fn fallback_task_downloaded_bytes(task: &StoreDownloadTask) -> u64 {
    let total_bytes = task_total_bytes(task);
    if total_bytes == 0 {
        return 0;
    }
    if task.status == "complete" {
        return total_bytes;
    }

    ((total_bytes as f64 * (task.progress.clamp(0.0, 100.0) / 100.0)).round() as u64)
        .min(total_bytes)
}

fn estimate_task_downloaded_bytes(progress_file: &ProgressFile, task: &StoreDownloadTask) -> u64 {
    if task.status == "complete" {
        return task_total_bytes(task);
    }

    progress_file
        .get_task_progress(&task.gid)
        .ok()
        .flatten()
        .map(|snapshot| snapshot.downloaded_bytes.min(snapshot.total_bytes))
        .unwrap_or_else(|| fallback_task_downloaded_bytes(task))
}

async fn hydrate_folder_aggregator_state(
    db: &DbHandle,
    progress_file: &ProgressFile,
    folder_aggregator: &FolderAggregator,
    parent: &StoreDownloadTask,
) -> Result<(), DmError> {
    let children = db.get_child_tasks(parent.gid.clone()).await?;
    let total_files = parent.total_files.unwrap_or(children.len() as i64);
    let completed_bytes = children
        .iter()
        .filter(|child| child.status == "complete")
        .map(task_total_bytes)
        .sum::<u64>();

    folder_aggregator.register_folder(
        &parent.gid,
        &parent.name,
        total_files,
        task_total_bytes(parent),
    );
    folder_aggregator.restore_counters(
        &parent.gid,
        parent.completed_files.unwrap_or(0),
        parent.failed_files.unwrap_or(0),
        completed_bytes,
    );

    for child in &children {
        if matches!(child.status.as_str(), "complete" | "removed") {
            continue;
        }

        folder_aggregator.register_child(&child.gid, &parent.gid);
        let downloaded_bytes = estimate_task_downloaded_bytes(progress_file, child);
        if downloaded_bytes > 0 {
            folder_aggregator.update_child_progress(&child.gid, downloaded_bytes);
        }
    }

    Ok(())
}

async fn persist_folder_progress(
    db: &DbHandle,
    folder_aggregator: &FolderAggregator,
    parent_gid: &str,
) {
    if let Some(snapshot) = folder_aggregator.get_progress(parent_gid)
        && let Err(e) = db
            .update_task(
                parent_gid.to_string(),
                TaskUpdate {
                    progress: Some(snapshot.progress),
                    ..TaskUpdate::default()
                },
            )
            .await
    {
        error!("[队列] 同步文件夹进度失败 {}: {}", parent_gid, e);
    }
}

/// 启动单个下载任务。
///
/// 内部会串联 URL 请求、状态切换、URL 刷新监控以及新任务/断点续传判定。
fn spawn_download_task(
    req: EnqueueRequest,
    completion_tx: mpsc::Sender<TaskCompletion>,
    signal_rx: watch::Receiver<DownloadSignal>,
    app: AppHandle,
    db: DbHandle,
    state_sync_notify: Arc<Notify>,
    url_resolver: Arc<UrlResolver>,
    progress_registry: Arc<ProgressRegistry>,
    http_client: reqwest::Client,
    progress_file: Arc<ProgressFile>,
    segment_semaphore: Arc<Semaphore>,
    conn_controller: Arc<ConnectionController>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let gid = req.gid.clone();

        // 1. 获取下载地址，同时监听暂停或取消信号。
        let url = tokio::select! {
            result = url_resolver.request_url(&app, &gid, &req.pick_code) => {
                match result {
                    Ok(url) => url,
                    Err(e) => {
                        error!("[队列] URL请求失败 gid={}: {}", gid, e);
                        let _ = db.update_task(gid.clone(), TaskUpdate {
                            status: Some("error".to_string()),
                            error_message: Some(Some(format!("请求下载地址失败：{}", e))),
                            ..TaskUpdate::default()
                        }).await;
                        state_sync_notify.notify_one();
                        let _ = completion_tx.send(TaskCompletion::Failed {
                            gid, error: format!("请求下载地址失败：{}", e),
                        }).await;
                        return;
                    }
                }
            }
            _ = async {
                let mut rx = signal_rx.clone();
                loop {
                    rx.changed().await.ok();
                    let sig = rx.borrow().clone();
                    if sig != DownloadSignal::Running {
                        break;
                    }
                }
            } => {
                // URL 获取期间若收到暂停或取消，直接结束本次启动。
                let sig = signal_rx.borrow().clone();
                let completion = if sig == DownloadSignal::Paused {
                    info!("[队列] URL请求期间任务暂停 gid={}", gid);
                    TaskCompletion::Paused { gid }
                } else {
                    info!("[队列] URL请求期间任务取消 gid={}", gid);
                    TaskCompletion::Cancelled { gid }
                };
                let _ = completion_tx.send(completion).await;
                return;
            }
        };

        // 2. 将数据库状态切换为 active。
        if let Err(e) = db
            .update_task(
                gid.clone(),
                TaskUpdate {
                    status: Some("active".to_string()),
                    ..TaskUpdate::default()
                },
            )
            .await
        {
            error!("[队列] 设置活跃状态失败 gid={}: {}", gid, e);
        }
        state_sync_notify.notify_one();

        // 3. 创建任务私有的 URL 广播通道，供后续地址刷新复用。
        let (url_tx, url_rx) = watch::channel(url.clone());
        let url_refresh_requested = Arc::new(AtomicBool::new(false));

        // 4. 启动 URL 刷新监控任务。
        let url_monitor = {
            let flag = url_refresh_requested.clone();
            let resolver = url_resolver.clone();
            let app_clone = app.clone();
            let gid_clone = gid.clone();
            let pick_code = req.pick_code.clone();
            let url_tx = url_tx.clone();
            let pf = progress_file.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if flag.load(Ordering::SeqCst) {
                        debug!("[队列] URL刷新触发 gid={}", gid_clone);
                        match resolver
                            .request_url(&app_clone, &gid_clone, &pick_code)
                            .await
                        {
                            Ok(new_url) => {
                                let _ = url_tx.send(new_url.clone());
                                // 将新 URL 持久化到 .oofp，避免崩溃恢复后继续使用旧地址。
                                if let Err(e) = pf.update_task_url(&gid_clone, &new_url) {
                                    warn!("[队列] URL持久化到.oofp失败 gid={}: {}", gid_clone, e);
                                }
                                flag.store(false, Ordering::SeqCst);
                                debug!("[队列] URL刷新成功 gid={}", gid_clone);
                            }
                            Err(e) => {
                                error!("[队列] URL刷新失败 gid={}: {}", gid_clone, e);
                                // 保留刷新标志，下次轮询继续尝试。
                            }
                        }
                    }
                }
            })
        };

        // 5. 根据 .oofp 是否存在决定走新下载还是断点续传。
        let config = DownloadConfig {
            split: req.split,
            speed_limit: 0,
        };

        let download_result = match progress_file.load_task(&req.save_path) {
            Ok(existing) if existing.task_id == gid => {
                // .oofp 存在且 task_id 匹配，按断点续传处理。
                info!("[队列] 从.oofp恢复下载 gid={}", gid);
                super::http::resume_download(
                    &http_client,
                    &gid,
                    &url,
                    &req.save_path,
                    &req.token,
                    &req.user_agent,
                    &config,
                    &progress_file,
                    &app,
                    signal_rx,
                    url_rx,
                    url_refresh_requested,
                    segment_semaphore,
                    conn_controller,
                    progress_registry.clone(),
                )
                .await
            }
            Ok(existing) => {
                // .oofp 存在但 task_id 不匹配：接管旧断点，允许删除后重新添加继续续传。
                info!(
                    "[队列] 发现旧.oofp gid={} 旧task_id={} — 复用断点继续下载",
                    gid, existing.task_id
                );
                match progress_file.rebind_task(
                    &req.save_path,
                    &existing.task_id,
                    &gid,
                    &req.name,
                    &url,
                    &req.pick_code,
                    req.expected_sha1.as_deref(),
                ) {
                    Ok(()) => {
                        super::http::resume_download(
                            &http_client,
                            &gid,
                            &url,
                            &req.save_path,
                            &req.token,
                            &req.user_agent,
                            &config,
                            &progress_file,
                            &app,
                            signal_rx,
                            url_rx,
                            url_refresh_requested,
                            segment_semaphore,
                            conn_controller,
                            progress_registry.clone(),
                        )
                        .await
                    }
                    Err(e) => {
                        error!("[队列] 接管旧.oofp失败 gid={}: {}", gid, e);
                        Err(e)
                    }
                }
            }
            Err(_) => {
                // 没有 .oofp，按全新下载处理。
                info!("[队列] 启动新下载 gid={}", gid);
                let mut task = super::types::DownloadTask {
                    task_id: gid.clone(),
                    file_name: req.name.clone(),
                    file_size: req.size as u64,
                    save_path: req.save_path.clone(),
                    url: url.clone(),
                    pick_code: req.pick_code.clone(),
                    etag: None,
                    expected_sha1: req.expected_sha1.clone(),
                    segments: Vec::new(),
                    status: super::types::TaskStatus::Pending,
                    created_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                };
                super::http::download_file(
                    &http_client,
                    &mut task,
                    &req.token,
                    &req.user_agent,
                    &config,
                    &progress_file,
                    &app,
                    signal_rx,
                    url_rx,
                    url_refresh_requested,
                    segment_semaphore,
                    conn_controller,
                    progress_registry.clone(),
                )
                .await
            }
        };

        // === 6. 终止 URL 监控 ===
        url_monitor.abort();

        // === 7. 映射下载结果到 TaskCompletion ===
        let completion = match download_result {
            Ok(()) => TaskCompletion::Completed { gid },
            Err(DownloadError::TaskAborted(TaskAbortReason::Paused)) => {
                TaskCompletion::Paused { gid }
            }
            Err(DownloadError::TaskAborted(TaskAbortReason::Cancelled)) => {
                TaskCompletion::Cancelled { gid }
            }
            Err(DownloadError::VerificationFailed(message)) => {
                warn!("[队列] 文件校验失败 gid={}: {}", gid, message);
                TaskCompletion::VerifyFailed { gid, message }
            }
            Err(e) => {
                error!("[队列] 下载失败 gid={}: {}", gid, e);
                TaskCompletion::Failed {
                    gid,
                    error: e.to_string(),
                }
            }
        };

        let _ = completion_tx.send(completion).await;
    })
}

// ==================== Tauri 命令 ====================

/// 入队单文件下载任务。
///
/// 依次创建数据库记录、压入内存队列，并触发一次状态同步。
#[tauri::command]
pub async fn download_enqueue_file(
    fid: String,
    name: String,
    pick_code: String,
    size: i64,
    save_path: String,
    expected_sha1: Option<String>,
    parent_gid: Option<String>,
    token: String,
    user_agent: String,
    split: u16,
    max_global_connections: u16,
    queue: tauri::State<'_, TaskQueue>,
    db: tauri::State<'_, DbHandle>,
    event_bridge: tauri::State<'_, EventBridge>,
) -> Result<String, DmError> {
    let gid = uuid::Uuid::new_v4().to_string();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    // 1. 先创建 waiting 状态的数据库记录。
    db.insert_task(StoreDownloadTask {
        gid: gid.clone(),
        fid: fid.clone(),
        name: name.clone(),
        pick_code: pick_code.clone(),
        size,
        status: "waiting".to_string(),
        progress: 0.0,
        path: Some(save_path.clone()),
        download_speed: 0,
        eta: None,
        error_message: None,
        error_code: None,
        created_at: Some(now_ms),
        completed_at: None,
        is_folder: false,
        is_collecting: false,
        parent_gid: parent_gid.clone(),
        total_files: None,
        completed_files: None,
        failed_files: None,
    })
    .await?;

    // 2. 再加入内存等待队列。
    queue
        .enqueue(EnqueueRequest {
            gid: gid.clone(),
            fid,
            name,
            pick_code,
            size,
            save_path,
            expected_sha1,
            parent_gid,
            token,
            user_agent,
            split,
            max_global_connections,
        })
        .await?;

    // 3. 触发一次状态同步，让前端立即看到新任务。
    event_bridge.notify_state_change();

    info!("[入队] 已入队 gid={}", gid);
    Ok(gid)
}

/// 文件夹下载入队。
///
/// 原子性创建父任务、批量子任务并一次性推入队列。
#[tauri::command]
pub async fn download_create_folder_task(
    parent_gid: String,
    parent_fid: String,
    parent_name: String,
    parent_pick_code: String,
    parent_path: String,
    db: tauri::State<'_, DbHandle>,
    event_bridge: tauri::State<'_, EventBridge>,
) -> Result<String, DmError> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    db.insert_task(StoreDownloadTask {
        gid: parent_gid.clone(),
        fid: parent_fid,
        name: parent_name.clone(),
        pick_code: parent_pick_code,
        size: 0,
        status: "active".to_string(),
        progress: 0.0,
        path: Some(parent_path),
        download_speed: 0,
        eta: None,
        error_message: None,
        error_code: None,
        created_at: Some(now_ms),
        completed_at: None,
        is_folder: true,
        is_collecting: true,
        parent_gid: None,
        total_files: Some(0),
        completed_files: Some(0),
        failed_files: Some(0),
    })
    .await?;

    event_bridge.notify_state_change();
    info!("[文件夹收集] 已创建 collecting 任务 gid={}", parent_gid);
    Ok(parent_gid)
}

#[tauri::command]
pub async fn download_restart_folder_collection(
    parent_gid: String,
    db: tauri::State<'_, DbHandle>,
    event_bridge: tauri::State<'_, EventBridge>,
) -> Result<(), DmError> {
    db.update_task(
        parent_gid.clone(),
        TaskUpdate {
            status: Some("active".to_string()),
            progress: Some(0.0),
            size: Some(0),
            download_speed: Some(0),
            eta: Some(None),
            error_message: Some(None),
            error_code: Some(None),
            completed_at: Some(None),
            is_folder: Some(true),
            is_collecting: Some(true),
            total_files: Some(Some(0)),
            completed_files: Some(Some(0)),
            failed_files: Some(Some(0)),
            ..TaskUpdate::default()
        },
    )
    .await?;

    event_bridge.notify_state_change();
    info!("[文件夹收集] 已重置 collecting 任务 gid={}", parent_gid);
    Ok(())
}

#[tauri::command]
pub async fn download_fail_folder_collection(
    parent_gid: String,
    error_message: String,
    db: tauri::State<'_, DbHandle>,
    event_bridge: tauri::State<'_, EventBridge>,
) -> Result<(), DmError> {
    db.update_task(
        parent_gid.clone(),
        TaskUpdate {
            status: Some("error".to_string()),
            progress: Some(0.0),
            download_speed: Some(0),
            eta: Some(None),
            error_message: Some(Some(error_message.clone())),
            error_code: Some(None),
            completed_at: Some(None),
            is_folder: Some(true),
            is_collecting: Some(false),
            ..TaskUpdate::default()
        },
    )
    .await?;

    event_bridge.notify_state_change();
    info!(
        "[文件夹收集] collecting 任务失败 gid={} error={}",
        parent_gid, error_message
    );
    Ok(())
}

#[tauri::command]
pub async fn download_enqueue_folder(
    parent_gid: String,
    parent_fid: String,
    parent_name: String,
    parent_pick_code: String,
    parent_path: String,
    files: Vec<FolderFileItem>,
    token: String,
    user_agent: String,
    split: u16,
    max_global_connections: u16,
    queue: tauri::State<'_, TaskQueue>,
    db: tauri::State<'_, DbHandle>,
    event_bridge: tauri::State<'_, EventBridge>,
) -> Result<String, DmError> {
    if files.len() > 50_000 {
        return Err(DmError::Internal(format!(
            "文件夹内文件过多：{}，上限为 50000 个",
            files.len()
        )));
    }
    for f in &files {
        if f.path.contains("..") || f.path.starts_with('/') || f.path.starts_with('\\') {
            return Err(DmError::Internal(format!("检测到非法子路径：{}", f.path)));
        }
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let total_files = files.len() as i64;
    let total_size: i64 = files.iter().map(|f| f.size).sum();
    let final_status = if files.is_empty() {
        "complete"
    } else {
        "active"
    };
    let final_progress = if files.is_empty() { 100.0 } else { 0.0 };
    let final_completed_at = if files.is_empty() { Some(now_ms) } else { None };

    // 1. 先创建或更新父文件夹任务。
    if db.get_task_by_gid(parent_gid.clone()).await?.is_some() {
        db.update_task(
            parent_gid.clone(),
            TaskUpdate {
                fid: Some(parent_fid.clone()),
                name: Some(parent_name.clone()),
                pick_code: Some(parent_pick_code.clone()),
                size: Some(total_size),
                status: Some(final_status.to_string()),
                progress: Some(final_progress),
                path: Some(Some(parent_path.clone())),
                download_speed: Some(0),
                eta: Some(None),
                error_message: Some(None),
                error_code: Some(None),
                completed_at: Some(final_completed_at),
                is_folder: Some(true),
                is_collecting: Some(false),
                total_files: Some(Some(total_files)),
                completed_files: Some(Some(0)),
                failed_files: Some(Some(0)),
                ..TaskUpdate::default()
            },
        )
        .await?;
    } else {
        db.insert_task(StoreDownloadTask {
            gid: parent_gid.clone(),
            fid: parent_fid,
            name: parent_name.clone(),
            pick_code: parent_pick_code,
            size: total_size,
            status: final_status.to_string(),
            progress: final_progress,
            path: Some(parent_path.clone()),
            download_speed: 0,
            eta: None,
            error_message: None,
            error_code: None,
            created_at: Some(now_ms),
            completed_at: final_completed_at,
            is_folder: true,
            is_collecting: false,
            parent_gid: None,
            total_files: Some(total_files),
            completed_files: Some(0),
            failed_files: Some(0),
        })
        .await?;
    }

    if files.is_empty() {
        std::fs::create_dir_all(&parent_path)
            .map_err(|e| DmError::Internal(format!("创建空文件夹失败: {}", e)))?;
        event_bridge.notify_state_change();
        info!(
            "[入队] 空文件夹已完成 gid={} path={}",
            parent_gid, parent_path
        );
        return Ok(parent_gid);
    }

    // 2. 批量构造子任务记录和对应的入队请求。
    let mut child_tasks = Vec::with_capacity(files.len());
    let mut enqueue_requests = Vec::with_capacity(files.len());
    let mut seen_paths: HashMap<String, usize> = HashMap::new();

    for file in &files {
        let child_gid = uuid::Uuid::new_v4().to_string();
        // 同名文件去重: 检测 save_path 冲突并添加后缀 (1), (2), ...
        let base_save_path = format!("{}/{}", parent_path, file.path);
        let save_path = match seen_paths.get_mut(&base_save_path) {
            Some(count) => {
                *count += 1;
                // 在扩展名前插入后缀, 如 "photo.jpg" → "photo (2).jpg"
                let path = &file.path;
                if let Some(dot_pos) = path.rfind('.') {
                    let stem = &path[..dot_pos];
                    let ext = &path[dot_pos..];
                    format!("{}/{} ({}){}", parent_path, stem, *count, ext)
                } else {
                    format!("{}/{} ({})", parent_path, path, *count)
                }
            }
            None => {
                seen_paths.insert(base_save_path, 1);
                format!("{}/{}", parent_path, file.path)
            }
        };

        child_tasks.push(StoreDownloadTask {
            gid: child_gid.clone(),
            fid: file.fid.clone(),
            name: file.name.clone(),
            pick_code: file.pick_code.clone(),
            size: file.size,
            status: "waiting".to_string(),
            progress: 0.0,
            path: Some(save_path.clone()),
            download_speed: 0,
            eta: None,
            error_message: None,
            error_code: None,
            created_at: Some(now_ms),
            completed_at: None,
            is_folder: false,
            is_collecting: false,
            parent_gid: Some(parent_gid.clone()),
            total_files: None,
            completed_files: None,
            failed_files: None,
        });

        enqueue_requests.push(EnqueueRequest {
            gid: child_gid,
            fid: file.fid.clone(),
            name: file.name.clone(),
            pick_code: file.pick_code.clone(),
            size: file.size,
            save_path,
            expected_sha1: None,
            parent_gid: Some(parent_gid.clone()),
            token: token.clone(),
            user_agent: user_agent.clone(),
            split,
            max_global_connections,
        });
    }

    db.batch_insert_tasks(child_tasks).await?;

    // 3. 在 FolderAggregator 中注册父文件夹。
    queue.folder_aggregator.register_folder(
        &parent_gid,
        &parent_name,
        total_files,
        total_size as u64,
    );

    // 4. 注册子任务并逐个入队。
    for req in enqueue_requests {
        queue
            .folder_aggregator
            .register_child(&req.gid, &parent_gid);
        queue.enqueue(req).await?;
    }

    // 5. Trigger state-sync
    event_bridge.notify_state_change();

    info!(
        "[入队] 文件夹已入队 gid={} 共{}个子任务",
        parent_gid, total_files
    );
    Ok(parent_gid)
}

/// 暂停文件夹下载 — 级联暂停所有子任务 (per FLD-05)
#[tauri::command]
pub async fn download_pause_folder(
    parent_gid: String,
    queue: tauri::State<'_, TaskQueue>,
) -> Result<(), DmError> {
    info!("[暂停文件夹] parent_gid={}", parent_gid);
    queue.pause_folder(parent_gid).await
}

/// 取消文件夹下载 — 级联取消所有子任务 + 删除 DB 记录 (per FLD-07)
#[tauri::command]
pub async fn download_cancel_folder(
    parent_gid: String,
    queue: tauri::State<'_, TaskQueue>,
) -> Result<(), DmError> {
    info!("[取消文件夹] parent_gid={}", parent_gid);
    queue.cancel_folder(parent_gid).await
}

/// 恢复文件夹下载 — 查询暂停子任务并批量恢复 (per FLD-06)
#[tauri::command]
pub async fn download_resume_folder(
    parent_gid: String,
    token: String,
    user_agent: String,
    split: u16,
    max_global_connections: u16,
    queue: tauri::State<'_, TaskQueue>,
    db: tauri::State<'_, DbHandle>,
    progress_file: tauri::State<'_, Arc<ProgressFile>>,
    event_bridge: tauri::State<'_, EventBridge>,
) -> Result<(), DmError> {
    info!("[恢复文件夹] parent_gid={}", parent_gid);

    let parent = db
        .get_task_by_gid(parent_gid.clone())
        .await?
        .ok_or_else(|| DmError::NotFound(format!("未找到文件夹任务 gid={}", parent_gid)))?;
    if !parent.is_folder {
        return Err(DmError::Internal(format!(
            "gid={} 不是文件夹任务",
            parent_gid
        )));
    }
    if parent.status != "paused" {
        return Err(DmError::Internal(format!(
            "文件夹任务 gid={} 当前状态为 '{}'，只有 paused 才能恢复",
            parent_gid, parent.status
        )));
    }

    let paused_children = db
        .get_child_tasks_by_status(parent_gid.clone(), "paused".to_string())
        .await?;

    for child in &paused_children {
        if let Err(e) = db
            .update_task(
                child.gid.clone(),
                TaskUpdate {
                    status: Some("waiting".to_string()),
                    ..TaskUpdate::default()
                },
            )
            .await
        {
            error!("[恢复文件夹] 重置子任务失败 {}: {}", child.gid, e);
        }
    }

    db.update_task(
        parent_gid.clone(),
        TaskUpdate {
            status: Some("active".to_string()),
            ..TaskUpdate::default()
        },
    )
    .await?;

    let children: Vec<EnqueueRequest> = paused_children
        .into_iter()
        .map(|child| EnqueueRequest {
            gid: child.gid,
            fid: child.fid,
            name: child.name,
            pick_code: child.pick_code,
            size: child.size,
            save_path: child.path.unwrap_or_default(),
            expected_sha1: None,
            parent_gid: child.parent_gid,
            token: token.clone(),
            user_agent: user_agent.clone(),
            split,
            max_global_connections,
        })
        .collect();

    hydrate_folder_aggregator_state(
        &db,
        progress_file.inner().as_ref(),
        queue.folder_aggregator.as_ref(),
        &parent,
    )
    .await?;

    queue.resume_folder_children(children).await?;
    event_bridge.notify_state_change();
    info!("[恢复文件夹] 文件夹{}已恢复", parent_gid);
    Ok(())
}

/// 重试文件夹中失败的子任务 (per FLD-08)
#[tauri::command]
pub async fn download_retry_folder(
    parent_gid: String,
    token: String,
    user_agent: String,
    split: u16,
    max_global_connections: u16,
    queue: tauri::State<'_, TaskQueue>,
    db: tauri::State<'_, DbHandle>,
    progress_file: tauri::State<'_, Arc<ProgressFile>>,
    event_bridge: tauri::State<'_, EventBridge>,
) -> Result<(), DmError> {
    info!("[重试文件夹] parent_gid={}", parent_gid);

    let parent = db
        .get_task_by_gid(parent_gid.clone())
        .await?
        .ok_or_else(|| DmError::NotFound(format!("未找到文件夹任务 gid={}", parent_gid)))?;
    if !parent.is_folder {
        return Err(DmError::Internal(format!(
            "gid={} 不是文件夹任务",
            parent_gid
        )));
    }
    if parent.status != "partial_error" && parent.status != "error" {
        return Err(DmError::Internal(format!(
            "文件夹任务 gid={} 当前状态为 '{}'，只有 partial_error 或 error 才能重试",
            parent_gid, parent.status
        )));
    }

    let mut failed_children = db
        .get_child_tasks_by_status(parent_gid.clone(), "error".to_string())
        .await?;
    // 同时纳入 verify_failed 的子任务，一并按失败重试处理。
    let verify_failed_children = db
        .get_child_tasks_by_status(parent_gid.clone(), "verify_failed".to_string())
        .await?;
    failed_children.extend(verify_failed_children);

    if failed_children.is_empty() {
        info!("[重试文件夹] 文件夹{}没有失败的子任务", parent_gid);
        return Ok(());
    }

    let retry_count = failed_children.len();

    for child in &failed_children {
        // SHA1 校验失败的子任务：删除已下载文件和 .oofp，强制从头下载
        if child.status == "verify_failed" {
            if let Some(ref path) = child.path {
                let _ = std::fs::remove_file(path);
                let _ = std::fs::remove_file(format!("{}.oofp", path));
            }
        }

        if let Err(e) = db
            .update_task(
                child.gid.clone(),
                TaskUpdate {
                    status: Some("waiting".to_string()),
                    error_message: Some(None),
                    error_code: Some(None),
                    progress: Some(0.0),
                    download_speed: Some(0),
                    eta: Some(None),
                    ..TaskUpdate::default()
                },
            )
            .await
        {
            error!("[重试文件夹] 重置子任务失败 {}: {}", child.gid, e);
        }
    }

    db.update_task(
        parent_gid.clone(),
        TaskUpdate {
            status: Some("active".to_string()),
            failed_files: Some(Some(0)),
            ..TaskUpdate::default()
        },
    )
    .await?;

    let children: Vec<EnqueueRequest> = failed_children
        .into_iter()
        .map(|child| EnqueueRequest {
            gid: child.gid,
            fid: child.fid,
            name: child.name,
            pick_code: child.pick_code,
            size: child.size,
            save_path: child.path.unwrap_or_default(),
            expected_sha1: None,
            parent_gid: child.parent_gid,
            token: token.clone(),
            user_agent: user_agent.clone(),
            split,
            max_global_connections,
        })
        .collect();

    let parent_for_hydrate = StoreDownloadTask {
        failed_files: Some(0),
        status: "active".to_string(),
        ..parent.clone()
    };
    hydrate_folder_aggregator_state(
        &db,
        progress_file.inner().as_ref(),
        queue.folder_aggregator.as_ref(),
        &parent_for_hydrate,
    )
    .await?;

    queue.retry_folder_children(children).await?;
    event_bridge.notify_state_change();
    info!(
        "[重试文件夹] 文件夹{}重试{}个失败子任务",
        parent_gid, retry_count
    );
    Ok(())
}

/// 动态调整最大并发数。
#[tauri::command]
pub fn download_set_max_concurrent(
    n: usize,
    queue: tauri::State<'_, TaskQueue>,
) -> Result<(), DmError> {
    queue.set_max_concurrent(n);
    info!("[设置最大并发] 设置为 {}", n.clamp(1, 5));
    Ok(())
}

/// 动态调整全局下载限速。
#[tauri::command]
pub fn download_set_speed_limit(bytes_per_sec: u64) {
    super::throttle::set_speed_limit(bytes_per_sec);
    info!("[设置限速] 设置为{} 字节/秒", bytes_per_sec);
}

/// 全部暂停。
///
/// 暂停所有运行中任务，并冻结等待队列，避免继续出队。
#[tauri::command]
pub async fn download_pause_all(queue: tauri::State<'_, TaskQueue>) -> Result<(), DmError> {
    info!("[全部暂停] 全部暂停");
    queue.pause_all().await
}

/// 全部继续。
///
/// 恢复所有暂停任务，并解除队列冻结。
#[tauri::command]
pub async fn download_resume_all(
    token: String,
    user_agent: String,
    split: u16,
    max_global_connections: u16,
    queue: tauri::State<'_, TaskQueue>,
) -> Result<(), DmError> {
    info!("[全部恢复] 全部恢复");
    queue
        .resume_all(token, user_agent, split, max_global_connections)
        .await
}

/// 暂停单个下载任务。
///
/// 活跃任务：发送暂停信号给引擎，引擎停止后释放并发槽位。
/// 等待中任务：直接从队列移除，DB 状态更新为 paused。
#[tauri::command]
pub async fn download_pause_task(
    gid: String,
    queue: tauri::State<'_, TaskQueue>,
) -> Result<(), DmError> {
    info!("[暂停任务] gid={}", gid);
    queue.pause(gid).await
}

/// 取消单个下载任务。
///
/// 活跃任务：发送取消信号，引擎停止后删除 DB 记录（磁盘文件和 `.oofp` 都保留）。
/// 等待中任务：直接从队列移除，DB 记录删除。
/// 这样 `.oofp` 可以像 `.aria2` 一样继续承担断点续传文件的职责。
#[tauri::command]
pub async fn download_cancel_task(
    gid: String,
    queue: tauri::State<'_, TaskQueue>,
) -> Result<(), DmError> {
    info!("[取消任务] gid={}", gid);
    queue.cancel(gid).await
}

/// 恢复暂停的下载任务。
///
/// 从 DB 读取任务信息，构造 EnqueueRequest，插入队列头部优先调度。
/// 出队后 spawn_download_task 会根据 .oofp 是否存在决定新下载还是断点续传。
#[tauri::command]
pub async fn download_resume_task(
    gid: String,
    token: String,
    user_agent: String,
    split: u16,
    max_global_connections: u16,
    queue: tauri::State<'_, TaskQueue>,
    db: tauri::State<'_, DbHandle>,
) -> Result<(), DmError> {
    info!("[恢复任务] gid={}", gid);

    let task = db
        .get_task_by_gid(gid.clone())
        .await?
        .ok_or_else(|| DmError::NotFound(format!("未找到下载任务 gid={}", gid)))?;

    if task.status != "paused" {
        return Err(DmError::Internal(format!(
            "下载任务 gid={} 当前状态为 '{}'，只有 paused 才能恢复",
            gid, task.status
        )));
    }

    db.update_task(
        gid.clone(),
        TaskUpdate {
            status: Some("waiting".to_string()),
            ..TaskUpdate::default()
        },
    )
    .await?;

    let req = EnqueueRequest {
        gid: task.gid,
        fid: task.fid,
        name: task.name,
        pick_code: task.pick_code,
        size: task.size,
        save_path: task.path.unwrap_or_default(),
        expected_sha1: None,
        parent_gid: task.parent_gid,
        token,
        user_agent,
        split,
        max_global_connections,
    };

    queue.resume(req).await
}

/// 重试失败的下载任务。
///
/// 重置数据库状态为 waiting，构造 EnqueueRequest，并插入队列尾部保持 FIFO。
#[tauri::command]
pub async fn download_retry_task(
    gid: String,
    token: String,
    user_agent: String,
    split: u16,
    max_global_connections: u16,
    queue: tauri::State<'_, TaskQueue>,
    db: tauri::State<'_, DbHandle>,
) -> Result<(), DmError> {
    info!("[重试任务] gid={}", gid);

    let task = db
        .get_task_by_gid(gid.clone())
        .await?
        .ok_or_else(|| DmError::NotFound(format!("未找到下载任务 gid={}", gid)))?;

    if task.status != "error" && task.status != "verify_failed" {
        return Err(DmError::Internal(format!(
            "下载任务 gid={} 当前状态为 '{}'，只有 error 或 verify_failed 才能重试",
            gid, task.status
        )));
    }

    // SHA1 校验失败时删除已下载文件和 .oofp，强制从头下载。
    // 否则 resume_download 会发现分片全完成，直接再次校验并重复失败。
    if task.status == "verify_failed" {
        if let Some(ref path) = task.path {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}.oofp", path));
        }
    }

    db.update_task(
        gid.clone(),
        TaskUpdate {
            status: Some("waiting".to_string()),
            error_message: Some(None),
            error_code: Some(None),
            progress: Some(0.0),
            download_speed: Some(0),
            eta: Some(None),
            ..TaskUpdate::default()
        },
    )
    .await?;

    let req = EnqueueRequest {
        gid: task.gid,
        fid: task.fid,
        name: task.name,
        pick_code: task.pick_code,
        size: task.size,
        save_path: task.path.unwrap_or_default(),
        expected_sha1: None,
        parent_gid: task.parent_gid,
        token,
        user_agent,
        split,
        max_global_connections,
    };

    queue.retry(req).await
}

// ==================== 启动恢复 ====================

/// 应用启动恢复流程。
///
/// 扫描未完成任务、检查 .oofp、修正数据库状态并重建文件夹映射。
async fn recover_tasks(
    db: &DbHandle,
    progress_file: &ProgressFile,
    folder_aggregator: &FolderAggregator,
    state_sync_notify: &Notify,
) {
    // 1. 查询所有可恢复任务，包括文件夹、子任务和单文件任务。
    let tasks = match db.get_recoverable_tasks().await {
        Ok(t) => t,
        Err(e) => {
            error!("[恢复] 查询可恢复任务失败: {e}");
            return;
        }
    };

    if tasks.is_empty() {
        info!("[恢复] 没有未完成的任务需要恢复");
        return;
    }

    info!("[恢复] 发现{}个未完成任务需要恢复", tasks.len());

    // 将文件夹任务和文件任务分开处理。
    let (folder_tasks, file_tasks): (Vec<_>, Vec<_>) = tasks.into_iter().partition(|t| t.is_folder);

    // 2. 逐个检查文件任务的 .oofp，并据此修正恢复状态。
    let mut task_statuses: HashMap<String, String> = HashMap::new();

    for task in &file_tasks {
        let oofp_ok = task
            .path
            .as_ref()
            .map(|p| progress_file.load_task(p).is_ok())
            .unwrap_or(false);

        let new_status = if oofp_ok { "paused" } else { "error" };
        task_statuses.insert(task.gid.clone(), new_status.to_string());

        if let Err(e) = db
            .update_task(
                task.gid.clone(),
                TaskUpdate {
                    status: Some(new_status.to_string()),
                    download_speed: Some(0),
                    eta: Some(None),
                    ..TaskUpdate::default()
                },
            )
            .await
        {
            error!("[恢复] 更新任务失败 {}: {e}", task.gid);
        }
    }

    // 3. 根据子任务状态推导父文件夹的恢复后状态，并重建文件夹聚合状态。
    for folder in &folder_tasks {
        if folder.is_collecting {
            if let Err(e) = db
                .update_task(
                    folder.gid.clone(),
                    TaskUpdate {
                        status: Some("error".to_string()),
                        download_speed: Some(0),
                        eta: Some(None),
                        error_message: Some(Some("文件夹文件收集已中断，请重试".to_string())),
                        error_code: Some(None),
                        is_collecting: Some(false),
                        ..TaskUpdate::default()
                    },
                )
                .await
            {
                error!("[恢复] 更新 collecting 文件夹失败 {}: {e}", folder.gid);
            }
            continue;
        }

        let has_error_child = file_tasks
            .iter()
            .filter(|t| t.parent_gid.as_deref() == Some(&*folder.gid))
            .any(|t| {
                task_statuses
                    .get(&t.gid)
                    .map(|s| s == "error")
                    .unwrap_or(false)
            });

        let folder_status = if has_error_child {
            "partial_error"
        } else {
            "paused"
        };

        if let Err(e) = db
            .update_task(
                folder.gid.clone(),
                TaskUpdate {
                    status: Some(folder_status.to_string()),
                    download_speed: Some(0),
                    eta: Some(None),
                    ..TaskUpdate::default()
                },
            )
            .await
        {
            error!("[恢复] 更新文件夹失败 {}: {e}", folder.gid);
        }

        let recovered_folder = StoreDownloadTask {
            status: folder_status.to_string(),
            ..folder.clone()
        };
        if let Err(e) =
            hydrate_folder_aggregator_state(db, progress_file, folder_aggregator, &recovered_folder)
                .await
        {
            error!("[恢复] 重建文件夹聚合状态失败 {}: {e}", folder.gid);
            continue;
        }
        persist_folder_progress(db, folder_aggregator, &folder.gid).await;
    }

    // Step 4: Notify state-sync so frontend gets the recovered task list
    state_sync_notify.notify_one();

    let file_count = file_tasks.len();
    let folder_count = folder_tasks.len();
    let error_count = task_statuses.values().filter(|s| *s == "error").count();
    info!("[恢复] 完成: {file_count}个文件任务 ({error_count}个错误), {folder_count}个文件夹任务");
}
