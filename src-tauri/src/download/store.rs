use rusqlite::Connection;
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};

const ERR_DB_ACTOR_CHANNEL_CLOSED: &str = "下载数据库不可用：请求通道已关闭";
const ERR_DB_ACTOR_REPLY_DROPPED: &str = "下载数据库不可用：响应通道已断开";

/// 反序列化 `Option<Option<T>>`。
///
/// JSON `null` 会变成 `Some(None)`，表示显式写入 SQL NULL；
/// 字段缺失则保持 `None`，表示本次更新不改动该列。
fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

// ==================== 错误类型 ====================

#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "type", content = "message")]
pub enum DmError {
    #[error("数据库错误: {0}")]
    DbError(String),
    #[error("未找到: {0}")]
    NotFound(String),
    #[error("内部错误: {0}")]
    Internal(String),
}

impl From<rusqlite::Error> for DmError {
    fn from(e: rusqlite::Error) -> Self {
        DmError::DbError(e.to_string())
    }
}

// ==================== 数据结构 ====================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub gid: String,
    pub fid: String,
    pub name: String,
    pub pick_code: String,
    pub size: i64,
    pub status: String,
    pub progress: f64,
    pub path: Option<String>,
    pub download_speed: i64,
    pub eta: Option<i64>,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub created_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub is_folder: bool,
    pub is_collecting: bool,
    pub parent_gid: Option<String>,
    pub total_files: Option<i64>,
    pub completed_files: Option<i64>,
    pub failed_files: Option<i64>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUpdate {
    pub fid: Option<String>,
    pub name: Option<String>,
    pub pick_code: Option<String>,
    pub size: Option<i64>,
    pub status: Option<String>,
    pub progress: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub path: Option<Option<String>>,
    pub download_speed: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub eta: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub error_message: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub error_code: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub created_at: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub completed_at: Option<Option<i64>>,
    pub is_folder: Option<bool>,
    pub is_collecting: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub parent_gid: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub total_files: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub completed_files: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub failed_files: Option<Option<i64>>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStats {
    pub active_count: i64,
    pub total_speed: i64,
    pub completed: i64,
    pub failed: i64,
    pub paused: i64,
    pub waiting: i64,
    pub total: i64,
}

// ==================== 数据库迁移 ====================
// 使用 PRAGMA user_version 追踪迁移版本，支持后续增量迁移。
// 规则：每个版本对应一个迁移步骤，只在新数据库或低版本时执行。

/// 当前数据库迁移版本（每次新增迁移时递增）
const DB_VERSION: u32 = 1;

/// 迁移步骤：(版本号, SQL)
const MIGRATIONS: &[(u32, &str)] = &[
    // v1: 创建 downloads 表
    (
        1,
        "CREATE TABLE IF NOT EXISTS downloads (
            gid TEXT PRIMARY KEY,
            fid TEXT NOT NULL,
            name TEXT NOT NULL,
            pick_code TEXT NOT NULL,
            size INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'active',
            progress REAL NOT NULL DEFAULT 0,
            path TEXT,
            download_speed INTEGER NOT NULL DEFAULT 0,
            eta INTEGER,
            error_message TEXT,
            error_code TEXT,
            created_at INTEGER,
            completed_at INTEGER,
            is_folder INTEGER NOT NULL DEFAULT 0,
            is_collecting INTEGER NOT NULL DEFAULT 0,
            parent_gid TEXT,
            total_files INTEGER,
            completed_files INTEGER,
            failed_files INTEGER
        );",
    ),
    // v2: 示例 — 新增字段时在此添加
    // (2, "ALTER TABLE downloads ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;"),
];

// ==================== Helper Functions ====================

fn row_to_task(row: &rusqlite::Row) -> Result<DownloadTask, rusqlite::Error> {
    Ok(DownloadTask {
        gid: row.get("gid")?,
        fid: row.get("fid")?,
        name: row.get("name")?,
        pick_code: row.get("pick_code")?,
        size: row.get("size")?,
        status: row.get("status")?,
        progress: row.get("progress")?,
        path: row.get("path")?,
        download_speed: row.get("download_speed")?,
        eta: row.get("eta")?,
        error_message: row.get("error_message")?,
        error_code: row.get("error_code")?,
        created_at: row.get("created_at")?,
        completed_at: row.get("completed_at")?,
        is_folder: row.get::<_, i32>("is_folder")? != 0,
        is_collecting: row.get::<_, i32>("is_collecting")? != 0,
        parent_gid: row.get("parent_gid")?,
        total_files: row.get("total_files")?,
        completed_files: row.get("completed_files")?,
        failed_files: row.get("failed_files")?,
    })
}

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

/// 执行版本化迁移：只运行当前 user_version 之后的迁移步骤
fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let current_version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current_version >= DB_VERSION {
        log::debug!("[数据库] 迁移版本已是最新 v{}", current_version);
        return Ok(());
    }
    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            log::info!("[数据库] 执行迁移 v{}...", version);
            conn.execute_batch(sql)?;
        }
    }
    conn.pragma_update(None, "user_version", DB_VERSION)?;
    log::info!("[数据库] 迁移完成 v{} → v{}", current_version, DB_VERSION);
    Ok(())
}

// ==================== DB Actor ====================

enum DbRequest {
    InsertTask {
        task: DownloadTask,
        reply: oneshot::Sender<Result<(), DmError>>,
    },
    BatchInsertTasks {
        tasks: Vec<DownloadTask>,
        reply: oneshot::Sender<Result<(), DmError>>,
    },
    UpdateTask {
        gid: String,
        updates: TaskUpdate,
        reply: oneshot::Sender<Result<(), DmError>>,
    },
    DeleteTask {
        gid: String,
        reply: oneshot::Sender<Result<(), DmError>>,
    },
    DeleteChildTasks {
        parent_gid: String,
        reply: oneshot::Sender<Result<(), DmError>>,
    },
    DeleteFinishedTasks {
        reply: oneshot::Sender<Result<u64, DmError>>,
    },
    GetTopLevelTasks {
        reply: oneshot::Sender<Result<Vec<DownloadTask>, DmError>>,
    },
    GetAllTasks {
        reply: oneshot::Sender<Result<Vec<DownloadTask>, DmError>>,
    },
    GetTaskByGid {
        gid: String,
        reply: oneshot::Sender<Result<Option<DownloadTask>, DmError>>,
    },
    GetChildTasks {
        parent_gid: String,
        reply: oneshot::Sender<Result<Vec<DownloadTask>, DmError>>,
    },
    GetChildTasksByStatus {
        parent_gid: String,
        status: String,
        reply: oneshot::Sender<Result<Vec<DownloadTask>, DmError>>,
    },
    IncrementFolderCounter {
        gid: String,
        field: String,
        delta: i64,
        reply: oneshot::Sender<Result<(), DmError>>,
    },
    GetIncompleteTasks {
        reply: oneshot::Sender<Result<Vec<DownloadTask>, DmError>>,
    },
    GetActiveGids {
        reply: oneshot::Sender<Result<Vec<String>, DmError>>,
    },
    HasActiveTasks {
        reply: oneshot::Sender<Result<bool, DmError>>,
    },
    GetDownloadStats {
        reply: oneshot::Sender<Result<DownloadStats, DmError>>,
    },
    GetPausedTopLevelTasks {
        reply: oneshot::Sender<Result<Vec<DownloadTask>, DmError>>,
    },
    GetRecoverableTasks {
        reply: oneshot::Sender<Result<Vec<DownloadTask>, DmError>>,
    },
}

#[derive(Clone)]
pub struct DbHandle {
    tx: mpsc::Sender<DbRequest>,
}

impl DbHandle {
    pub fn new(db_path: String) -> Result<Self, DmError> {
        let (tx, mut rx) = mpsc::channel::<DbRequest>(256);

        // 先在主线程做一次预检，避免初始化错误被延后到后台线程 panic。
        init_connection(&db_path)?;

        let worker_db_path = db_path.clone();

        std::thread::spawn(move || {
            let conn = init_connection(&worker_db_path)
                .unwrap_or_else(|err| panic!("初始化下载数据库工作线程失败：{}", err));

            while let Some(req) = rx.blocking_recv() {
                match req {
                    DbRequest::InsertTask { task, reply } => {
                        let _ = reply.send(insert_task_impl(&conn, &task));
                    }
                    DbRequest::BatchInsertTasks { tasks, reply } => {
                        let _ = reply.send(batch_insert_tasks_impl(&conn, &tasks));
                    }
                    DbRequest::UpdateTask {
                        gid,
                        updates,
                        reply,
                    } => {
                        let _ = reply.send(update_task_impl(&conn, &gid, &updates));
                    }
                    DbRequest::DeleteTask { gid, reply } => {
                        let _ = reply.send(delete_task_impl(&conn, &gid));
                    }
                    DbRequest::DeleteChildTasks { parent_gid, reply } => {
                        let _ = reply.send(delete_child_tasks_impl(&conn, &parent_gid));
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
                    DbRequest::GetTaskByGid { gid, reply } => {
                        let _ = reply.send(get_task_by_gid_impl(&conn, &gid));
                    }
                    DbRequest::GetChildTasks { parent_gid, reply } => {
                        let _ = reply.send(get_child_tasks_impl(&conn, &parent_gid));
                    }
                    DbRequest::GetChildTasksByStatus {
                        parent_gid,
                        status,
                        reply,
                    } => {
                        let _ =
                            reply.send(get_child_tasks_by_status_impl(&conn, &parent_gid, &status));
                    }
                    DbRequest::IncrementFolderCounter {
                        gid,
                        field,
                        delta,
                        reply,
                    } => {
                        let _ =
                            reply.send(increment_folder_counter_impl(&conn, &gid, &field, delta));
                    }
                    DbRequest::GetIncompleteTasks { reply } => {
                        let _ = reply.send(get_incomplete_tasks_impl(&conn));
                    }
                    DbRequest::GetActiveGids { reply } => {
                        let _ = reply.send(get_active_gids_impl(&conn));
                    }
                    DbRequest::HasActiveTasks { reply } => {
                        let _ = reply.send(has_active_tasks_impl(&conn));
                    }
                    DbRequest::GetDownloadStats { reply } => {
                        let _ = reply.send(get_download_stats_impl(&conn));
                    }
                    DbRequest::GetPausedTopLevelTasks { reply } => {
                        let _ = reply.send(get_paused_top_level_tasks_impl(&conn));
                    }
                    DbRequest::GetRecoverableTasks { reply } => {
                        let _ = reply.send(get_recoverable_tasks_impl(&conn));
                    }
                }
            }
        });

        Ok(Self { tx })
    }

    async fn send_request<T>(
        &self,
        req_fn: impl FnOnce(oneshot::Sender<Result<T, DmError>>) -> DbRequest,
    ) -> Result<T, DmError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(req_fn(tx))
            .await
            .map_err(|_| DmError::Internal(ERR_DB_ACTOR_CHANNEL_CLOSED.into()))?;
        rx.await
            .map_err(|_| DmError::Internal(ERR_DB_ACTOR_REPLY_DROPPED.into()))?
    }

    pub async fn insert_task(&self, task: DownloadTask) -> Result<(), DmError> {
        self.send_request(|reply| DbRequest::InsertTask { task, reply })
            .await
    }

    pub async fn batch_insert_tasks(&self, tasks: Vec<DownloadTask>) -> Result<(), DmError> {
        self.send_request(|reply| DbRequest::BatchInsertTasks { tasks, reply })
            .await
    }

    pub async fn update_task(&self, gid: String, updates: TaskUpdate) -> Result<(), DmError> {
        self.send_request(|reply| DbRequest::UpdateTask {
            gid,
            updates,
            reply,
        })
        .await
    }

    pub async fn delete_task(&self, gid: String) -> Result<(), DmError> {
        self.send_request(|reply| DbRequest::DeleteTask { gid, reply })
            .await
    }

    pub async fn delete_child_tasks(&self, parent_gid: String) -> Result<(), DmError> {
        self.send_request(|reply| DbRequest::DeleteChildTasks { parent_gid, reply })
            .await
    }

    pub async fn delete_finished_tasks(&self) -> Result<u64, DmError> {
        self.send_request(|reply| DbRequest::DeleteFinishedTasks { reply })
            .await
    }

    pub async fn get_top_level_tasks(&self) -> Result<Vec<DownloadTask>, DmError> {
        self.send_request(|reply| DbRequest::GetTopLevelTasks { reply })
            .await
    }

    pub async fn get_all_tasks(&self) -> Result<Vec<DownloadTask>, DmError> {
        self.send_request(|reply| DbRequest::GetAllTasks { reply })
            .await
    }

    pub async fn get_task_by_gid(&self, gid: String) -> Result<Option<DownloadTask>, DmError> {
        self.send_request(|reply| DbRequest::GetTaskByGid { gid, reply })
            .await
    }

    pub async fn get_child_tasks(&self, parent_gid: String) -> Result<Vec<DownloadTask>, DmError> {
        self.send_request(|reply| DbRequest::GetChildTasks { parent_gid, reply })
            .await
    }

    pub async fn get_child_tasks_by_status(
        &self,
        parent_gid: String,
        status: String,
    ) -> Result<Vec<DownloadTask>, DmError> {
        self.send_request(|reply| DbRequest::GetChildTasksByStatus {
            parent_gid,
            status,
            reply,
        })
        .await
    }

    pub async fn increment_folder_counter(
        &self,
        gid: String,
        field: String,
        delta: i64,
    ) -> Result<(), DmError> {
        self.send_request(|reply| DbRequest::IncrementFolderCounter {
            gid,
            field,
            delta,
            reply,
        })
        .await
    }

    pub async fn get_incomplete_tasks(&self) -> Result<Vec<DownloadTask>, DmError> {
        self.send_request(|reply| DbRequest::GetIncompleteTasks { reply })
            .await
    }

    pub async fn get_recoverable_tasks(&self) -> Result<Vec<DownloadTask>, DmError> {
        self.send_request(|reply| DbRequest::GetRecoverableTasks { reply })
            .await
    }

    pub async fn get_active_gids(&self) -> Result<Vec<String>, DmError> {
        self.send_request(|reply| DbRequest::GetActiveGids { reply })
            .await
    }

    pub async fn has_active_tasks(&self) -> Result<bool, DmError> {
        self.send_request(|reply| DbRequest::HasActiveTasks { reply })
            .await
    }

    pub async fn get_download_stats(&self) -> Result<DownloadStats, DmError> {
        self.send_request(|reply| DbRequest::GetDownloadStats { reply })
            .await
    }

    pub async fn get_paused_top_level_tasks(&self) -> Result<Vec<DownloadTask>, DmError> {
        self.send_request(|reply| DbRequest::GetPausedTopLevelTasks { reply })
            .await
    }
}

// ==================== DB Implementation Functions ====================

fn insert_task_impl(conn: &Connection, task: &DownloadTask) -> Result<(), DmError> {
    conn.execute(
        "INSERT OR REPLACE INTO downloads (
            gid, fid, name, pick_code, size, status, progress, path,
            download_speed, eta, error_message, error_code,
            created_at, completed_at, is_folder, is_collecting,
            parent_gid, total_files, completed_files, failed_files
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        rusqlite::params![
            task.gid,
            task.fid,
            task.name,
            task.pick_code,
            task.size,
            task.status,
            task.progress,
            task.path,
            task.download_speed,
            task.eta,
            task.error_message,
            task.error_code,
            task.created_at,
            task.completed_at,
            task.is_folder as i32,
            task.is_collecting as i32,
            task.parent_gid,
            task.total_files,
            task.completed_files,
            task.failed_files,
        ],
    )?;
    Ok(())
}

fn batch_insert_tasks_impl(conn: &Connection, tasks: &[DownloadTask]) -> Result<(), DmError> {
    let tx = conn.unchecked_transaction()?;
    for task in tasks {
        insert_task_impl(&tx, task)?;
    }
    tx.commit()?;
    Ok(())
}

fn update_task_impl(conn: &Connection, gid: &str, updates: &TaskUpdate) -> Result<(), DmError> {
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

    add_field!(updates.fid, "fid");
    add_field!(updates.name, "name");
    add_field!(updates.pick_code, "pick_code");
    add_field!(updates.size, "size");
    add_field!(updates.status, "status");
    add_field!(updates.progress, "progress");
    add_nullable_field!(updates.path, "path");
    add_field!(updates.download_speed, "download_speed");
    add_nullable_field!(updates.eta, "eta");
    add_nullable_field!(updates.error_message, "error_message");
    add_nullable_field!(updates.error_code, "error_code");
    add_nullable_field!(updates.created_at, "created_at");
    add_nullable_field!(updates.completed_at, "completed_at");
    add_bool_field!(updates.is_folder, "is_folder");
    add_bool_field!(updates.is_collecting, "is_collecting");
    add_nullable_field!(updates.parent_gid, "parent_gid");
    add_nullable_field!(updates.total_files, "total_files");
    add_nullable_field!(updates.completed_files, "completed_files");
    add_nullable_field!(updates.failed_files, "failed_files");

    if set_clauses.is_empty() {
        return Ok(());
    }

    let sql = format!(
        "UPDATE downloads SET {} WHERE gid = ?{}",
        set_clauses.join(", "),
        idx
    );
    params.push(Box::new(gid.to_string()));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows_affected = conn.execute(&sql, param_refs.as_slice())?;
    if rows_affected == 0 {
        return Err(DmError::NotFound(format!("task gid={} not found", gid)));
    }
    Ok(())
}

fn delete_task_impl(conn: &Connection, gid: &str) -> Result<(), DmError> {
    conn.execute(
        "DELETE FROM downloads WHERE gid = ?1",
        rusqlite::params![gid],
    )?;
    Ok(())
}

fn delete_child_tasks_impl(conn: &Connection, parent_gid: &str) -> Result<(), DmError> {
    conn.execute(
        "DELETE FROM downloads WHERE parent_gid = ?1",
        rusqlite::params![parent_gid],
    )?;
    Ok(())
}

fn delete_finished_tasks_impl(conn: &Connection) -> Result<u64, DmError> {
    let tx = conn.unchecked_transaction()?;

    // Get folder gids that are finished
    let folder_gids: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT gid FROM downloads WHERE is_folder = 1 AND status IN ('complete', 'error', 'partial_error', 'verify_failed', 'removed')",
        )?;
        stmt.query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?
    };

    // Delete child tasks of finished folders
    for gid in &folder_gids {
        tx.execute(
            "DELETE FROM downloads WHERE parent_gid = ?1",
            rusqlite::params![gid],
        )?;
    }

    // Delete top-level finished tasks
    let deleted = tx.execute(
        "DELETE FROM downloads WHERE parent_gid IS NULL AND status IN ('complete', 'error', 'partial_error', 'verify_failed', 'removed')",
        [],
    )?;

    tx.commit()?;
    Ok(deleted as u64)
}

fn get_top_level_tasks_impl(conn: &Connection) -> Result<Vec<DownloadTask>, DmError> {
    let mut stmt =
        conn.prepare("SELECT * FROM downloads WHERE parent_gid IS NULL ORDER BY created_at DESC")?;
    let tasks = stmt
        .query_map([], |row| row_to_task(row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

fn get_all_tasks_impl(conn: &Connection) -> Result<Vec<DownloadTask>, DmError> {
    let mut stmt = conn.prepare("SELECT * FROM downloads ORDER BY created_at DESC")?;
    let tasks = stmt
        .query_map([], |row| row_to_task(row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

fn get_task_by_gid_impl(conn: &Connection, gid: &str) -> Result<Option<DownloadTask>, DmError> {
    let mut stmt = conn.prepare("SELECT * FROM downloads WHERE gid = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![gid], |row| row_to_task(row))?;
    match rows.next() {
        Some(Ok(task)) => Ok(Some(task)),
        Some(Err(e)) => Err(DmError::from(e)),
        None => Ok(None),
    }
}

fn get_child_tasks_impl(conn: &Connection, parent_gid: &str) -> Result<Vec<DownloadTask>, DmError> {
    let mut stmt =
        conn.prepare("SELECT * FROM downloads WHERE parent_gid = ?1 ORDER BY created_at ASC")?;
    let tasks = stmt
        .query_map(rusqlite::params![parent_gid], |row| row_to_task(row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

fn get_child_tasks_by_status_impl(
    conn: &Connection,
    parent_gid: &str,
    status: &str,
) -> Result<Vec<DownloadTask>, DmError> {
    let mut stmt = conn.prepare(
        "SELECT * FROM downloads WHERE parent_gid = ?1 AND status = ?2 ORDER BY created_at ASC",
    )?;
    let tasks = stmt
        .query_map(rusqlite::params![parent_gid, status], |row| {
            row_to_task(row)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

fn increment_folder_counter_impl(
    conn: &Connection,
    gid: &str,
    field: &str,
    delta: i64,
) -> Result<(), DmError> {
    // Whitelist allowed field names to prevent SQL injection
    let column = match field {
        "completed_files" => "completed_files",
        "failed_files" => "failed_files",
        _ => {
            return Err(DmError::Internal(format!(
                "Invalid counter field: {}",
                field
            )));
        }
    };
    let sql = format!(
        "UPDATE downloads SET {} = COALESCE({}, 0) + ?1 WHERE gid = ?2",
        column, column
    );
    let rows = conn.execute(&sql, rusqlite::params![delta, gid])?;
    if rows == 0 {
        return Err(DmError::NotFound(format!("task gid={} not found", gid)));
    }
    Ok(())
}

fn get_incomplete_tasks_impl(conn: &Connection) -> Result<Vec<DownloadTask>, DmError> {
    let mut stmt = conn.prepare(
        "SELECT * FROM downloads
         WHERE is_folder = 0
           AND gid NOT LIKE 'failed-%'
           AND gid NOT LIKE 'folder-%'
           AND status NOT IN ('complete', 'error', 'removed')
         ORDER BY created_at ASC",
    )?;
    let tasks = stmt
        .query_map([], |row| row_to_task(row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

fn get_recoverable_tasks_impl(conn: &Connection) -> Result<Vec<DownloadTask>, DmError> {
    let mut stmt = conn.prepare(
        "SELECT * FROM downloads
         WHERE gid NOT LIKE 'failed-%'
           AND status NOT IN ('complete', 'error', 'removed')
         ORDER BY created_at ASC",
    )?;
    let tasks = stmt
        .query_map([], |row| row_to_task(row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

fn get_active_gids_impl(conn: &Connection) -> Result<Vec<String>, DmError> {
    let mut stmt = conn.prepare(
        "SELECT gid FROM downloads
         WHERE is_folder = 0
           AND gid NOT LIKE 'failed-%'
           AND gid NOT LIKE 'folder-%'
           AND status IN ('active', 'waiting', 'paused')",
    )?;
    let gids = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(gids)
}

fn has_active_tasks_impl(conn: &Connection) -> Result<bool, DmError> {
    let mut stmt = conn.prepare(
        "SELECT COUNT(*) FROM downloads
         WHERE (is_folder = 0 AND gid NOT LIKE 'failed-%' AND status IN ('active', 'waiting', 'paused'))
            OR (is_folder = 1 AND is_collecting = 1)",
    )?;
    let count: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(count > 0)
}

fn get_download_stats_impl(conn: &Connection) -> Result<DownloadStats, DmError> {
    let mut stmt = conn.prepare(
        "SELECT status, COUNT(*) as cnt, COALESCE(SUM(download_speed), 0) as total_speed
         FROM downloads
         WHERE parent_gid IS NULL
         GROUP BY status",
    )?;

    let mut stats = DownloadStats {
        active_count: 0,
        total_speed: 0,
        completed: 0,
        failed: 0,
        paused: 0,
        waiting: 0,
        total: 0,
    };

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;

    for row in rows {
        let (status, cnt, total_speed) = row?;
        stats.total += cnt;
        match status.as_str() {
            "active" => {
                stats.active_count = cnt;
                stats.total_speed = total_speed;
            }
            "complete" => stats.completed = cnt,
            "error" => stats.failed = cnt,
            "paused" => stats.paused = cnt,
            "waiting" => stats.waiting = cnt,
            _ => {}
        }
    }

    Ok(stats)
}

fn get_paused_top_level_tasks_impl(conn: &Connection) -> Result<Vec<DownloadTask>, DmError> {
    let mut stmt = conn.prepare(
        "SELECT * FROM downloads WHERE parent_gid IS NULL AND status = 'paused' ORDER BY created_at ASC",
    )?;
    let tasks = stmt
        .query_map([], |row| row_to_task(row))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

// ==================== Tauri Commands ====================

#[tauri::command]
pub async fn download_insert_task(
    task: DownloadTask,
    db: tauri::State<'_, DbHandle>,
) -> Result<(), DmError> {
    db.insert_task(task).await
}

#[tauri::command]
pub async fn download_batch_insert_tasks(
    tasks: Vec<DownloadTask>,
    db: tauri::State<'_, DbHandle>,
) -> Result<(), DmError> {
    db.batch_insert_tasks(tasks).await
}

#[tauri::command]
pub async fn download_update_task(
    gid: String,
    updates: TaskUpdate,
    db: tauri::State<'_, DbHandle>,
) -> Result<(), DmError> {
    db.update_task(gid, updates).await
}

#[tauri::command]
pub async fn download_delete_task(
    gid: String,
    db: tauri::State<'_, DbHandle>,
) -> Result<(), DmError> {
    db.delete_task(gid).await
}

#[tauri::command]
pub async fn download_delete_child_tasks(
    parent_gid: String,
    db: tauri::State<'_, DbHandle>,
) -> Result<(), DmError> {
    db.delete_child_tasks(parent_gid).await
}

#[tauri::command]
pub async fn download_delete_finished_tasks(
    db: tauri::State<'_, DbHandle>,
) -> Result<u64, DmError> {
    db.delete_finished_tasks().await
}

#[tauri::command]
pub async fn download_get_top_level_tasks(
    db: tauri::State<'_, DbHandle>,
) -> Result<Vec<DownloadTask>, DmError> {
    db.get_top_level_tasks().await
}

#[tauri::command]
pub async fn download_get_all_tasks(
    db: tauri::State<'_, DbHandle>,
) -> Result<Vec<DownloadTask>, DmError> {
    db.get_all_tasks().await
}

#[tauri::command]
pub async fn download_get_task_by_gid(
    gid: String,
    db: tauri::State<'_, DbHandle>,
) -> Result<Option<DownloadTask>, DmError> {
    db.get_task_by_gid(gid).await
}

#[tauri::command]
pub async fn download_get_child_tasks(
    parent_gid: String,
    db: tauri::State<'_, DbHandle>,
) -> Result<Vec<DownloadTask>, DmError> {
    db.get_child_tasks(parent_gid).await
}

#[tauri::command]
pub async fn download_get_incomplete_tasks(
    db: tauri::State<'_, DbHandle>,
) -> Result<Vec<DownloadTask>, DmError> {
    db.get_incomplete_tasks().await
}

#[tauri::command]
pub async fn download_get_active_gids(
    db: tauri::State<'_, DbHandle>,
) -> Result<Vec<String>, DmError> {
    db.get_active_gids().await
}

#[tauri::command]
pub async fn download_has_active_tasks(db: tauri::State<'_, DbHandle>) -> Result<bool, DmError> {
    db.has_active_tasks().await
}

#[tauri::command]
pub async fn download_get_download_stats(
    db: tauri::State<'_, DbHandle>,
) -> Result<DownloadStats, DmError> {
    db.get_download_stats().await
}
