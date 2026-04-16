//! 上传任务存储层。
//!
//! 这一层只负责两件事：
//! - 用 SQLite 持久化上传任务，保证重启后还能恢复列表状态
//! - 用 actor 线程串行化数据库访问，避免在异步上下文里直接共享 rusqlite 连接

use log::info;
use rusqlite::Connection;
use serde::Deserialize;
use tauri::{App, Manager};
use tokio::sync::{mpsc, oneshot};

use super::sync::UploadStateSync;

const ERR_DB_ACTOR_CHANNEL_CLOSED: &str = "上传数据库不可用：请求通道已关闭";
const ERR_DB_ACTOR_REPLY_DROPPED: &str = "上传数据库不可用：响应通道已断开";

/// 让 `TaskUpdate` 支持三态字段语义：
/// - `None`: 不更新该字段
/// - `Some(None)`: 显式清空该字段
/// - `Some(Some(value))`: 显式写入新值
fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

/// 存储层统一错误类型。
#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "type", content = "message")]
pub enum UploadStoreError {
    #[error("数据库错误: {0}")]
    DbError(String),
    #[error("未找到: {0}")]
    NotFound(String),
    #[error("内部错误: {0}")]
    Internal(String),
}

impl From<rusqlite::Error> for UploadStoreError {
    fn from(err: rusqlite::Error) -> Self {
        Self::DbError(err.to_string())
    }
}

/// 上传任务的完整持久化模型。
///
/// 这个结构既是数据库行模型，也是前端 `upload:state-sync` 事件载荷。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadTask {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_size: i64,
    pub target_cid: String,
    pub target_path: Option<String>,
    pub sha1: Option<String>,
    pub pre_sha1: Option<String>,
    pub pick_code: Option<String>,
    pub status: String,
    pub progress: f64,
    pub error_message: Option<String>,
    pub created_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub is_folder: bool,
    pub parent_id: Option<String>,
    pub total_files: Option<i64>,
    pub completed_files: Option<i64>,
    pub failed_files: Option<i64>,
    pub oss_bucket: Option<String>,
    pub oss_object: Option<String>,
    pub oss_endpoint: Option<String>,
    pub callback: Option<String>,
    pub callback_var: Option<String>,
    pub uploaded_size: i64,
    pub file_id: Option<String>,
    pub oss_upload_id: Option<String>,
}

/// 上传任务的部分更新补丁。
///
/// 调度器通常只会更新其中几个字段，因此这里不用完整 `UploadTask` 覆盖写回。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUpdate {
    pub file_name: Option<String>,
    pub file_path: Option<String>,
    pub file_size: Option<i64>,
    pub target_cid: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub target_path: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub sha1: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub pre_sha1: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub pick_code: Option<Option<String>>,
    pub status: Option<String>,
    pub progress: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub error_message: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub created_at: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub completed_at: Option<Option<i64>>,
    pub is_folder: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub parent_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub total_files: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub completed_files: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub failed_files: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub oss_bucket: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub oss_object: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub oss_endpoint: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub callback: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub callback_var: Option<Option<String>>,
    pub uploaded_size: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub file_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub oss_upload_id: Option<Option<String>>,
}

/// 当前数据库 schema 版本。
const DB_VERSION: u32 = 1;

/// 迁移脚本列表，按版本从小到大执行。
const MIGRATIONS: &[(u32, &str)] = &[(
    1,
    "CREATE TABLE IF NOT EXISTS uploads (
      id TEXT PRIMARY KEY,
      file_name TEXT NOT NULL,
      file_path TEXT NOT NULL,
      file_size INTEGER NOT NULL DEFAULT 0,
      target_cid TEXT NOT NULL DEFAULT '0',
      target_path TEXT,
      sha1 TEXT,
      pre_sha1 TEXT,
      pick_code TEXT,
      status TEXT NOT NULL DEFAULT 'pending',
      progress REAL NOT NULL DEFAULT 0,
      error_message TEXT,
      created_at INTEGER,
      completed_at INTEGER,
      is_folder INTEGER NOT NULL DEFAULT 0,
      parent_id TEXT,
      total_files INTEGER,
      completed_files INTEGER,
      failed_files INTEGER,
      oss_bucket TEXT,
      oss_object TEXT,
      oss_endpoint TEXT,
      callback TEXT,
      callback_var TEXT,
      uploaded_size INTEGER NOT NULL DEFAULT 0,
      file_id TEXT,
      oss_upload_id TEXT
  );",
)];

/// 把 SQLite 行映射成内存中的 `UploadTask`。
fn row_to_task(row: &rusqlite::Row) -> Result<UploadTask, rusqlite::Error> {
    Ok(UploadTask {
        id: row.get("id")?,
        file_name: row.get("file_name")?,
        file_path: row.get("file_path")?,
        file_size: row.get("file_size")?,
        target_cid: row.get("target_cid")?,
        target_path: row.get("target_path")?,
        sha1: row.get("sha1")?,
        pre_sha1: row.get("pre_sha1")?,
        pick_code: row.get("pick_code")?,
        status: row.get("status")?,
        progress: row.get("progress")?,
        error_message: row.get("error_message")?,
        created_at: row.get("created_at")?,
        completed_at: row.get("completed_at")?,
        is_folder: row.get::<_, i32>("is_folder")? != 0,
        parent_id: row.get("parent_id")?,
        total_files: row.get("total_files")?,
        completed_files: row.get("completed_files")?,
        failed_files: row.get("failed_files")?,
        oss_bucket: row.get("oss_bucket")?,
        oss_object: row.get("oss_object")?,
        oss_endpoint: row.get("oss_endpoint")?,
        callback: row.get("callback")?,
        callback_var: row.get("callback_var")?,
        uploaded_size: row.get("uploaded_size")?,
        file_id: row.get("file_id")?,
        oss_upload_id: row.get("oss_upload_id")?,
    })
}

/// 初始化数据库连接并配置运行参数。
fn init_connection(db_path: &str) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA busy_timeout=5000;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;",
    )?;
    run_migrations(&conn)?;
    Ok(conn)
}

/// 执行数据库迁移直到当前版本。
fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let current_version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current_version >= DB_VERSION {
        return Ok(());
    }

    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            info!("[上传数据库] 执行迁移 v{}", version);
            conn.execute_batch(sql)?;
        }
    }

    conn.pragma_update(None, "user_version", DB_VERSION)?;
    info!(
        "[上传数据库] 迁移完成 from=v{} to=v{}",
        current_version, DB_VERSION
    );
    Ok(())
}

/// 发往数据库 actor 线程的请求。
enum DbRequest {
    InsertTask {
        task: UploadTask,
        reply: oneshot::Sender<Result<(), UploadStoreError>>,
    },
    UpdateTask {
        id: String,
        updates: TaskUpdate,
        reply: oneshot::Sender<Result<(), UploadStoreError>>,
    },
    DeleteTask {
        id: String,
        reply: oneshot::Sender<Result<(), UploadStoreError>>,
    },
    DeleteChildTasks {
        parent_id: String,
        reply: oneshot::Sender<Result<(), UploadStoreError>>,
    },
    DeleteFinishedTasks {
        reply: oneshot::Sender<Result<u64, UploadStoreError>>,
    },
    GetTopLevelTasks {
        reply: oneshot::Sender<Result<Vec<UploadTask>, UploadStoreError>>,
    },
    GetAllTasks {
        reply: oneshot::Sender<Result<Vec<UploadTask>, UploadStoreError>>,
    },
    GetTaskById {
        id: String,
        reply: oneshot::Sender<Result<Option<UploadTask>, UploadStoreError>>,
    },
    GetChildTasks {
        parent_id: String,
        reply: oneshot::Sender<Result<Vec<UploadTask>, UploadStoreError>>,
    },
}

/// 数据库 actor 的异步句柄。
///
/// 业务线程只通过 channel 与数据库线程通信，不直接持有 rusqlite 连接。
#[derive(Clone)]
pub struct DbHandle {
    tx: mpsc::Sender<DbRequest>,
}

impl DbHandle {
    /// 创建数据库 actor，并启动专用工作线程。
    pub fn new(db_path: String) -> Result<Self, UploadStoreError> {
        let (tx, mut rx) = mpsc::channel::<DbRequest>(256);

        info!("[上传数据库] 初始化数据库 path={}", db_path);
        init_connection(&db_path)?;

        let worker_db_path = db_path.clone();
        std::thread::spawn(move || {
            let conn = init_connection(&worker_db_path)
                .unwrap_or_else(|err| panic!("初始化上传数据库工作线程失败：{}", err));

            while let Some(req) = rx.blocking_recv() {
                match req {
                    DbRequest::InsertTask { task, reply } => {
                        let _ = reply.send(insert_task_impl(&conn, &task));
                    }
                    DbRequest::UpdateTask { id, updates, reply } => {
                        let _ = reply.send(update_task_impl(&conn, &id, &updates));
                    }
                    DbRequest::DeleteTask { id, reply } => {
                        let _ = reply.send(delete_task_impl(&conn, &id));
                    }
                    DbRequest::DeleteChildTasks { parent_id, reply } => {
                        let _ = reply.send(delete_child_tasks_impl(&conn, &parent_id));
                    }
                    DbRequest::DeleteFinishedTasks { reply } => {
                        let _ = reply.send(delete_finished_tasks_impl(&conn));
                    }
                    DbRequest::GetTopLevelTasks { reply } => {
                        let _ = reply.send(get_top_level_tasks_impl(&conn));
                    }
                    DbRequest::GetAllTasks { reply } => {
                        let _ = reply.send(get_all_tasks_impl(&conn));
                    }
                    DbRequest::GetTaskById { id, reply } => {
                        let _ = reply.send(get_task_by_id_impl(&conn, &id));
                    }
                    DbRequest::GetChildTasks { parent_id, reply } => {
                        let _ = reply.send(get_child_tasks_impl(&conn, &parent_id));
                    }
                }
            }
        });

        info!("[上传数据库] 数据库已就绪 path={}", db_path);

        Ok(Self { tx })
    }

    /// 统一发送请求到数据库 actor 并等待响应。
    async fn send_request<T>(
        &self,
        req_fn: impl FnOnce(oneshot::Sender<Result<T, UploadStoreError>>) -> DbRequest,
    ) -> Result<T, UploadStoreError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(req_fn(tx))
            .await
            .map_err(|_| UploadStoreError::Internal(ERR_DB_ACTOR_CHANNEL_CLOSED.into()))?;
        rx.await
            .map_err(|_| UploadStoreError::Internal(ERR_DB_ACTOR_REPLY_DROPPED.into()))?
    }

    pub async fn insert_task(&self, task: UploadTask) -> Result<(), UploadStoreError> {
        self.send_request(|reply| DbRequest::InsertTask { task, reply })
            .await
    }

    pub async fn update_task(
        &self,
        id: String,
        updates: TaskUpdate,
    ) -> Result<(), UploadStoreError> {
        self.send_request(|reply| DbRequest::UpdateTask { id, updates, reply })
            .await
    }

    pub async fn delete_task(&self, id: String) -> Result<(), UploadStoreError> {
        self.send_request(|reply| DbRequest::DeleteTask { id, reply })
            .await
    }

    pub async fn delete_child_tasks(&self, parent_id: String) -> Result<(), UploadStoreError> {
        self.send_request(|reply| DbRequest::DeleteChildTasks { parent_id, reply })
            .await
    }

    pub async fn delete_finished_tasks(&self) -> Result<u64, UploadStoreError> {
        self.send_request(|reply| DbRequest::DeleteFinishedTasks { reply })
            .await
    }

    pub async fn get_top_level_tasks(&self) -> Result<Vec<UploadTask>, UploadStoreError> {
        self.send_request(|reply| DbRequest::GetTopLevelTasks { reply })
            .await
    }

    pub async fn get_all_tasks(&self) -> Result<Vec<UploadTask>, UploadStoreError> {
        self.send_request(|reply| DbRequest::GetAllTasks { reply })
            .await
    }

    pub async fn get_task_by_id(&self, id: String) -> Result<Option<UploadTask>, UploadStoreError> {
        self.send_request(|reply| DbRequest::GetTaskById { id, reply })
            .await
    }

    pub async fn get_child_tasks(
        &self,
        parent_id: String,
    ) -> Result<Vec<UploadTask>, UploadStoreError> {
        self.send_request(|reply| DbRequest::GetChildTasks { parent_id, reply })
            .await
    }
}

fn insert_task_impl(conn: &Connection, task: &UploadTask) -> Result<(), UploadStoreError> {
    conn.execute(
        "INSERT OR REPLACE INTO uploads (
            id, file_name, file_path, file_size, target_cid, target_path,
            sha1, pre_sha1, pick_code, status, progress,
            error_message, created_at, completed_at, is_folder, parent_id,
            total_files, completed_files, failed_files,
            oss_bucket, oss_object, oss_endpoint, callback, callback_var,
            uploaded_size, file_id, oss_upload_id
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11,
            ?12, ?13, ?14, ?15, ?16,
            ?17, ?18, ?19,
            ?20, ?21, ?22, ?23, ?24,
            ?25, ?26, ?27
        )",
        rusqlite::params![
            task.id,
            task.file_name,
            task.file_path,
            task.file_size,
            task.target_cid,
            task.target_path,
            task.sha1,
            task.pre_sha1,
            task.pick_code,
            task.status,
            task.progress,
            task.error_message,
            task.created_at,
            task.completed_at,
            task.is_folder as i32,
            task.parent_id,
            task.total_files,
            task.completed_files,
            task.failed_files,
            task.oss_bucket,
            task.oss_object,
            task.oss_endpoint,
            task.callback,
            task.callback_var,
            task.uploaded_size,
            task.file_id,
            task.oss_upload_id,
        ],
    )?;
    Ok(())
}

/// 根据 `TaskUpdate` 动态拼装 SQL 更新语句。
///
/// 这样可以只更新调用方真正关心的列，避免覆盖掉并发流程刚写入的新状态。
fn update_task_impl(
    conn: &Connection,
    id: &str,
    updates: &TaskUpdate,
) -> Result<(), UploadStoreError> {
    let mut set_clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1u32;

    macro_rules! add_field {
        ($field:expr, $col:expr) => {
            if let Some(ref val) = $field {
                set_clauses.push(format!("{} = ?{}", $col, idx));
                params.push(Box::new(val.clone()));
                idx += 1;
            }
        };
    }

    macro_rules! add_nullable_field {
        ($field:expr, $col:expr) => {
            if let Some(ref val) = $field {
                set_clauses.push(format!("{} = ?{}", $col, idx));
                params.push(Box::new(val.clone()));
                idx += 1;
            }
        };
    }

    macro_rules! add_bool_field {
        ($field:expr, $col:expr) => {
            if let Some(val) = $field {
                set_clauses.push(format!("{} = ?{}", $col, idx));
                params.push(Box::new(val as i32));
                idx += 1;
            }
        };
    }

    add_field!(updates.file_name, "file_name");
    add_field!(updates.file_path, "file_path");
    add_field!(updates.file_size, "file_size");
    add_field!(updates.target_cid, "target_cid");
    add_nullable_field!(updates.target_path, "target_path");
    add_nullable_field!(updates.sha1, "sha1");
    add_nullable_field!(updates.pre_sha1, "pre_sha1");
    add_nullable_field!(updates.pick_code, "pick_code");
    add_field!(updates.status, "status");
    add_field!(updates.progress, "progress");
    add_nullable_field!(updates.error_message, "error_message");
    add_nullable_field!(updates.created_at, "created_at");
    add_nullable_field!(updates.completed_at, "completed_at");
    add_bool_field!(updates.is_folder, "is_folder");
    add_nullable_field!(updates.parent_id, "parent_id");
    add_nullable_field!(updates.total_files, "total_files");
    add_nullable_field!(updates.completed_files, "completed_files");
    add_nullable_field!(updates.failed_files, "failed_files");
    add_nullable_field!(updates.oss_bucket, "oss_bucket");
    add_nullable_field!(updates.oss_object, "oss_object");
    add_nullable_field!(updates.oss_endpoint, "oss_endpoint");
    add_nullable_field!(updates.callback, "callback");
    add_nullable_field!(updates.callback_var, "callback_var");
    add_field!(updates.uploaded_size, "uploaded_size");
    add_nullable_field!(updates.file_id, "file_id");
    add_nullable_field!(updates.oss_upload_id, "oss_upload_id");

    if set_clauses.is_empty() {
        return Ok(());
    }

    let sql = format!(
        "UPDATE uploads SET {} WHERE id = ?{}",
        set_clauses.join(", "),
        idx
    );
    params.push(Box::new(id.to_string()));
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows_affected = conn.execute(&sql, param_refs.as_slice())?;
    if rows_affected == 0 {
        return Err(UploadStoreError::NotFound(format!(
            "task id={} not found",
            id
        )));
    }
    Ok(())
}

/// 删除单条上传任务记录。
fn delete_task_impl(conn: &Connection, id: &str) -> Result<(), UploadStoreError> {
    conn.execute("DELETE FROM uploads WHERE id = ?1", rusqlite::params![id])?;
    Ok(())
}

/// 删除某个文件夹任务下的所有子任务。
fn delete_child_tasks_impl(conn: &Connection, parent_id: &str) -> Result<(), UploadStoreError> {
    conn.execute(
        "DELETE FROM uploads WHERE parent_id = ?1",
        rusqlite::params![parent_id],
    )?;
    Ok(())
}

/// 删除所有已结束的顶层任务，并连带清理它们的子任务。
fn delete_finished_tasks_impl(conn: &Connection) -> Result<u64, UploadStoreError> {
    let tx = conn.unchecked_transaction()?;

    let folder_ids: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM uploads WHERE is_folder = 1 AND status IN ('complete', 'error', 'cancelled')",
        )?;
        stmt.query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?
    };

    for id in &folder_ids {
        tx.execute(
            "DELETE FROM uploads WHERE parent_id = ?1",
            rusqlite::params![id],
        )?;
    }

    let deleted = tx.execute(
        "DELETE FROM uploads WHERE parent_id IS NULL AND status IN ('complete', 'error', 'cancelled')",
        [],
    )?;

    tx.commit()?;
    Ok(deleted as u64)
}

/// 查询主列表展示用的顶层任务。
fn get_top_level_tasks_impl(conn: &Connection) -> Result<Vec<UploadTask>, UploadStoreError> {
    let mut stmt =
        conn.prepare("SELECT * FROM uploads WHERE parent_id IS NULL ORDER BY created_at DESC")?;
    let tasks = stmt
        .query_map([], |row| row_to_task(row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

/// 查询数据库中的全部任务记录。
fn get_all_tasks_impl(conn: &Connection) -> Result<Vec<UploadTask>, UploadStoreError> {
    let mut stmt = conn.prepare("SELECT * FROM uploads ORDER BY created_at DESC")?;
    let tasks = stmt
        .query_map([], |row| row_to_task(row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

/// 按 id 查询单个任务。
fn get_task_by_id_impl(
    conn: &Connection,
    id: &str,
) -> Result<Option<UploadTask>, UploadStoreError> {
    let mut stmt = conn.prepare("SELECT * FROM uploads WHERE id = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![id], |row| row_to_task(row))?;
    match rows.next() {
        Some(Ok(task)) => Ok(Some(task)),
        Some(Err(err)) => Err(UploadStoreError::from(err)),
        None => Ok(None),
    }
}

/// 查询某个文件夹任务下的所有子任务。
fn get_child_tasks_impl(
    conn: &Connection,
    parent_id: &str,
) -> Result<Vec<UploadTask>, UploadStoreError> {
    let mut stmt =
        conn.prepare("SELECT * FROM uploads WHERE parent_id = ?1 ORDER BY created_at ASC")?;
    let tasks = stmt
        .query_map(rusqlite::params![parent_id], |row| row_to_task(row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

/// 上传存储模块初始化阶段的错误。
#[derive(Debug, thiserror::Error)]
pub enum UploadInitError {
    #[error("无法解析应用数据目录：{0}")]
    ResolveAppDataDir(String),
    #[error("无法创建应用数据目录 {path}：{source}")]
    CreateAppDataDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("无法初始化上传数据库：{0}")]
    InitDatabase(#[from] UploadStoreError),
}

/// 初始化上传数据库并注册进 Tauri 全局状态。
pub fn init(app: &App) -> Result<(), UploadInitError> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| UploadInitError::ResolveAppDataDir(err.to_string()))?;
    std::fs::create_dir_all(&app_data_dir).map_err(|source| UploadInitError::CreateAppDataDir {
        path: app_data_dir.display().to_string(),
        source,
    })?;

    let db_path = app_data_dir.join("uploads.db");
    let db_handle = DbHandle::new(db_path.to_string_lossy().to_string())?;

    app.manage(db_handle);
    Ok(())
}

/// 删除所有已结束的上传任务。
#[tauri::command]
pub async fn upload_delete_finished_tasks(
    db: tauri::State<'_, DbHandle>,
    sync: tauri::State<'_, UploadStateSync>,
) -> Result<u64, UploadStoreError> {
    let deleted = db.delete_finished_tasks().await?;
    info!("[上传数据库] 清理已结束任务 deleted={}", deleted);
    sync.notify_state_change();
    Ok(deleted)
}

/// 获取主列表使用的顶层上传任务。
#[tauri::command]
pub async fn upload_get_top_level_tasks(
    db: tauri::State<'_, DbHandle>,
) -> Result<Vec<UploadTask>, UploadStoreError> {
    db.get_top_level_tasks().await
}
