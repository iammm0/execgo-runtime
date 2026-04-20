use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    types::{
        ErrorCode, EventRecord, EventType, ExecutionSpec, ResourceLimits, ResourceUsage,
        RuntimeErrorInfo, SandboxPolicy, SubmitTaskRequest, TaskStatus,
    },
};

#[derive(Debug, Clone)]
pub struct Repository {
    db_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub task_id: String,
    pub handle_id: String,
    pub status: TaskStatus,
    pub execution: ExecutionSpec,
    pub limits: ResourceLimits,
    pub sandbox: SandboxPolicy,
    pub metadata: std::collections::BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub shim_pid: Option<u32>,
    pub pid: Option<u32>,
    pub pgid: Option<i32>,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<i32>,
    pub error_code: Option<ErrorCode>,
    pub error: Option<RuntimeErrorInfo>,
    pub usage: Option<ResourceUsage>,
    pub task_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub request_path: PathBuf,
    pub result_path: PathBuf,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
    pub script_path: Option<PathBuf>,
    pub stdout_max_bytes: u64,
    pub stderr_max_bytes: u64,
    pub kill_requested: bool,
    pub kill_requested_at: Option<DateTime<Utc>>,
    pub timeout_triggered: bool,
    pub result_json: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct NewTaskRecord {
    pub task_id: String,
    pub request: SubmitTaskRequest,
    pub task_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub request_path: PathBuf,
    pub result_path: PathBuf,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
    pub script_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CompletionUpdate {
    pub status: TaskStatus,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<i32>,
    pub error: Option<RuntimeErrorInfo>,
    pub usage: Option<ResourceUsage>,
    pub result_json: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    pub by_status: std::collections::BTreeMap<String, u64>,
    pub by_error_code: std::collections::BTreeMap<String, u64>,
    pub finished_durations_ms: Vec<u64>,
}

impl Repository {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn init(&self) -> AppResult<()> {
        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = self.connect()?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS tasks (
                task_id TEXT PRIMARY KEY,
                handle_id TEXT NOT NULL,
                status TEXT NOT NULL,
                execution_json TEXT NOT NULL,
                limits_json TEXT NOT NULL,
                sandbox_json TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                started_at_ms INTEGER NULL,
                finished_at_ms INTEGER NULL,
                duration_ms INTEGER NULL,
                shim_pid INTEGER NULL,
                pid INTEGER NULL,
                pgid INTEGER NULL,
                exit_code INTEGER NULL,
                exit_signal INTEGER NULL,
                error_code TEXT NULL,
                error_json TEXT NULL,
                usage_json TEXT NULL,
                task_dir TEXT NOT NULL,
                workspace_dir TEXT NOT NULL,
                request_path TEXT NOT NULL,
                result_path TEXT NOT NULL,
                stdout_path TEXT NOT NULL,
                stderr_path TEXT NOT NULL,
                script_path TEXT NULL,
                stdout_max_bytes INTEGER NOT NULL,
                stderr_max_bytes INTEGER NOT NULL,
                kill_requested INTEGER NOT NULL DEFAULT 0,
                kill_requested_at_ms INTEGER NULL,
                timeout_triggered INTEGER NOT NULL DEFAULT 0,
                result_json TEXT NULL
            );

            CREATE TABLE IF NOT EXISTS task_events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                message TEXT NULL,
                data_json TEXT NULL,
                FOREIGN KEY(task_id) REFERENCES tasks(task_id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_tasks_status_created ON tasks(status, created_at_ms);
            CREATE INDEX IF NOT EXISTS idx_tasks_finished_at ON tasks(finished_at_ms);
            CREATE INDEX IF NOT EXISTS idx_task_events_task_id_seq ON task_events(task_id, seq);
            "#,
        )?;
        Ok(())
    }

    pub fn insert_task(&self, new_task: &NewTaskRecord) -> AppResult<()> {
        let now = Utc::now();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            r#"
            INSERT INTO tasks (
                task_id, handle_id, status,
                execution_json, limits_json, sandbox_json, metadata_json,
                created_at_ms, updated_at_ms,
                task_dir, workspace_dir, request_path, result_path, stdout_path, stderr_path, script_path,
                stdout_max_bytes, stderr_max_bytes
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
            )
            "#,
            params![
                new_task.task_id,
                new_task.task_id,
                encode_status(TaskStatus::Accepted),
                to_json(&new_task.request.execution)?,
                to_json(&new_task.request.limits)?,
                to_json(&new_task.request.sandbox)?,
                to_json(&new_task.request.metadata)?,
                now.timestamp_millis(),
                now.timestamp_millis(),
                new_task.task_dir.to_string_lossy().to_string(),
                new_task.workspace_dir.to_string_lossy().to_string(),
                new_task.request_path.to_string_lossy().to_string(),
                new_task.result_path.to_string_lossy().to_string(),
                new_task.stdout_path.to_string_lossy().to_string(),
                new_task.stderr_path.to_string_lossy().to_string(),
                new_task
                    .script_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                i64::try_from(new_task.request.limits.stdout_max_bytes)
                    .map_err(|_| AppError::InvalidInput("stdout_max_bytes is too large".into()))?,
                i64::try_from(new_task.request.limits.stderr_max_bytes)
                    .map_err(|_| AppError::InvalidInput("stderr_max_bytes is too large".into()))?,
            ],
        )
        .map_err(|err| {
            if let rusqlite::Error::SqliteFailure(code, _) = &err {
                if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY {
                    return AppError::Conflict(format!("task {} already exists", new_task.task_id));
                }
            }
            AppError::Sqlite(err)
        })?;
        insert_event_tx(
            &tx,
            &new_task.task_id,
            EventType::Submitted,
            Some("task submitted"),
            None,
        )?;
        insert_event_tx(
            &tx,
            &new_task.task_id,
            EventType::Accepted,
            Some("task accepted"),
            None,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_task(&self, task_id: &str) -> AppResult<TaskRecord> {
        let conn = self.connect()?;
        let task = conn
            .query_row(
                "SELECT * FROM tasks WHERE task_id = ?1",
                params![task_id],
                row_to_task_record,
            )
            .optional()?;
        task.ok_or_else(|| AppError::NotFound(task_id.to_string()))
    }

    pub fn list_events(&self, task_id: &str) -> AppResult<Vec<EventRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT seq, task_id, event_type, timestamp_ms, message, data_json FROM task_events WHERE task_id = ?1 ORDER BY seq ASC",
        )?;
        let iter = stmt.query_map(params![task_id], |row| {
            Ok(EventRecord {
                seq: row.get(0)?,
                task_id: row.get(1)?,
                event_type: decode_event_type(row.get::<_, String>(2)?.as_str())?,
                timestamp: ts_millis_to_utc(row.get(3)?),
                message: row.get(4)?,
                data: opt_json_value(row.get(5)?)?,
            })
        })?;
        let mut events = Vec::new();
        for item in iter {
            events.push(item?);
        }
        Ok(events)
    }

    pub fn count_accepted(&self) -> AppResult<u64> {
        self.count_by_status(TaskStatus::Accepted)
    }

    pub fn count_running(&self) -> AppResult<u64> {
        self.count_by_status(TaskStatus::Running)
    }

    pub fn count_by_status(&self, status: TaskStatus) -> AppResult<u64> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = ?1",
            params![encode_status(status)],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn list_accepted(&self, limit: usize) -> AppResult<Vec<TaskRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM tasks WHERE status = 'accepted' ORDER BY created_at_ms ASC LIMIT ?1",
        )?;
        let iter = stmt.query_map(params![limit as i64], row_to_task_record)?;
        let mut items = Vec::new();
        for item in iter {
            items.push(item?);
        }
        Ok(items)
    }

    pub fn list_non_terminal(&self) -> AppResult<Vec<TaskRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM tasks WHERE status IN ('accepted', 'running') ORDER BY created_at_ms ASC",
        )?;
        let iter = stmt.query_map([], row_to_task_record)?;
        let mut items = Vec::new();
        for item in iter {
            items.push(item?);
        }
        Ok(items)
    }

    pub fn mark_dispatched(&self, task_id: &str, shim_pid: u32) -> AppResult<()> {
        let now = Utc::now().timestamp_millis();
        let conn = self.connect()?;
        conn.execute(
            "UPDATE tasks SET status = 'running', shim_pid = ?2, updated_at_ms = ?3 WHERE task_id = ?1 AND status = 'accepted'",
            params![task_id, i64::from(shim_pid), now],
        )?;
        Ok(())
    }

    pub fn mark_started(
        &self,
        task_id: &str,
        pid: u32,
        pgid: i32,
        script_path: Option<&Path>,
    ) -> AppResult<()> {
        let now = Utc::now();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE tasks SET status = 'running', pid = ?2, pgid = ?3, started_at_ms = ?4, updated_at_ms = ?4, script_path = COALESCE(?5, script_path) WHERE task_id = ?1",
            params![
                task_id,
                i64::from(pid),
                pgid,
                now.timestamp_millis(),
                script_path.map(|p| p.to_string_lossy().to_string())
            ],
        )?;
        insert_event_tx(&tx, task_id, EventType::Started, Some("task started"), None)?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_cancel_requested(&self, task_id: &str) -> AppResult<TaskRecord> {
        let now = Utc::now();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE tasks SET kill_requested = 1, kill_requested_at_ms = ?2, updated_at_ms = ?2 WHERE task_id = ?1",
            params![task_id, now.timestamp_millis()],
        )?;
        insert_event_tx(
            &tx,
            task_id,
            EventType::KillRequested,
            Some("kill requested"),
            None,
        )?;
        tx.commit()?;
        self.get_task(task_id)
    }

    pub fn mark_timeout_triggered(&self, task_id: &str) -> AppResult<()> {
        let now = Utc::now();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE tasks SET timeout_triggered = 1, updated_at_ms = ?2 WHERE task_id = ?1",
            params![task_id, now.timestamp_millis()],
        )?;
        insert_event_tx(
            &tx,
            task_id,
            EventType::TimeoutTriggered,
            Some("timeout triggered"),
            None,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn cancel_accepted_task(&self, task_id: &str, error: RuntimeErrorInfo) -> AppResult<()> {
        let now = Utc::now();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            r#"
            UPDATE tasks
            SET status = 'cancelled',
                updated_at_ms = ?2,
                finished_at_ms = ?2,
                error_code = ?3,
                error_json = ?4,
                duration_ms = 0,
                result_json = ?5
            WHERE task_id = ?1 AND status = 'accepted'
            "#,
            params![
                task_id,
                now.timestamp_millis(),
                encode_error_code(error.code),
                to_json(&error)?,
                to_json(&serde_json::json!({
                    "task_id": task_id,
                    "handle_id": task_id,
                    "status": TaskStatus::Cancelled,
                    "finished_at": now,
                    "error": error,
                }))?,
            ],
        )?;
        insert_event_tx(
            &tx,
            task_id,
            EventType::Cancelled,
            Some("task cancelled"),
            None,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn complete_task(&self, task_id: &str, update: &CompletionUpdate) -> AppResult<()> {
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            r#"
            UPDATE tasks
            SET status = ?2,
                updated_at_ms = ?3,
                finished_at_ms = ?3,
                duration_ms = ?4,
                exit_code = ?5,
                exit_signal = ?6,
                error_code = ?7,
                error_json = ?8,
                usage_json = ?9,
                result_json = ?10
            WHERE task_id = ?1
            "#,
            params![
                task_id,
                encode_status(update.status.clone()),
                update.finished_at.timestamp_millis(),
                update
                    .duration_ms
                    .map(i64::try_from)
                    .transpose()
                    .map_err(|_| {
                        AppError::InvalidInput("duration_ms is too large to persist".into())
                    })?,
                update.exit_code,
                update.exit_signal,
                update.error.as_ref().map(|e| encode_error_code(e.code)),
                update.error.as_ref().map(to_json).transpose()?,
                update.usage.as_ref().map(to_json).transpose()?,
                update.result_json.as_ref().map(to_json).transpose()?,
            ],
        )?;

        let event_type = match update.status {
            TaskStatus::Success => EventType::Finished,
            TaskStatus::Failed => EventType::Failed,
            TaskStatus::Cancelled => EventType::Cancelled,
            TaskStatus::Accepted | TaskStatus::Running => EventType::Finished,
        };
        let message = match update.status {
            TaskStatus::Success => Some("task finished"),
            TaskStatus::Failed => Some("task failed"),
            TaskStatus::Cancelled => Some("task cancelled"),
            TaskStatus::Accepted | TaskStatus::Running => Some("task finished"),
        };
        insert_event_tx(&tx, task_id, event_type, message, None)?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_recovered(&self, task_id: &str) -> AppResult<()> {
        let now = Utc::now();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE tasks SET updated_at_ms = ?2 WHERE task_id = ?1 AND status = 'running'",
            params![task_id, now.timestamp_millis()],
        )?;
        insert_event_tx(
            &tx,
            task_id,
            EventType::Recovered,
            Some("task recovered"),
            None,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_recovery_lost(&self, task_id: &str) -> AppResult<()> {
        let update = CompletionUpdate {
            status: TaskStatus::Failed,
            finished_at: Utc::now(),
            duration_ms: Some(0),
            exit_code: None,
            exit_signal: None,
            error: Some(RuntimeErrorInfo {
                code: ErrorCode::Internal,
                message: "recovery_lost".into(),
                details: None,
            }),
            usage: None,
            result_json: None,
        };
        self.complete_task(task_id, &update)
    }

    pub fn is_cancel_requested(&self, task_id: &str) -> AppResult<bool> {
        let conn = self.connect()?;
        let flag: i64 = conn.query_row(
            "SELECT kill_requested FROM tasks WHERE task_id = ?1",
            params![task_id],
            |row| row.get(0),
        )?;
        Ok(flag != 0)
    }

    pub fn list_gc_candidates(&self, finished_before: DateTime<Utc>) -> AppResult<Vec<TaskRecord>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM tasks WHERE status IN ('success', 'failed', 'cancelled') AND finished_at_ms IS NOT NULL AND finished_at_ms <= ?1 ORDER BY finished_at_ms ASC",
        )?;
        let iter = stmt.query_map(
            params![finished_before.timestamp_millis()],
            row_to_task_record,
        )?;
        let mut items = Vec::new();
        for item in iter {
            items.push(item?);
        }
        Ok(items)
    }

    pub fn delete_task(&self, task_id: &str) -> AppResult<()> {
        let conn = self.connect()?;
        conn.execute("DELETE FROM tasks WHERE task_id = ?1", params![task_id])?;
        Ok(())
    }

    pub fn metrics_snapshot(&self) -> AppResult<MetricsSnapshot> {
        let conn = self.connect()?;
        let mut snapshot = MetricsSnapshot::default();

        let mut status_stmt = conn.prepare("SELECT status, COUNT(*) FROM tasks GROUP BY status")?;
        let status_rows = status_stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for item in status_rows {
            let (status, count) = item?;
            snapshot.by_status.insert(status, count.max(0) as u64);
        }

        let mut err_stmt = conn.prepare(
            "SELECT error_code, COUNT(*) FROM tasks WHERE error_code IS NOT NULL GROUP BY error_code",
        )?;
        let err_rows = err_stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for item in err_rows {
            let (code, count) = item?;
            snapshot.by_error_code.insert(code, count.max(0) as u64);
        }

        let mut duration_stmt =
            conn.prepare("SELECT duration_ms FROM tasks WHERE duration_ms IS NOT NULL")?;
        let duration_rows = duration_stmt.query_map([], |row| row.get::<_, i64>(0))?;
        for item in duration_rows {
            snapshot.finished_durations_ms.push(item?.max(0) as u64);
        }

        Ok(snapshot)
    }

    fn connect(&self) -> AppResult<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Ok(conn)
    }
}

pub fn generate_task_id() -> String {
    Uuid::new_v4().to_string()
}

fn row_to_task_record(row: &Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        task_id: row.get("task_id")?,
        handle_id: row.get("handle_id")?,
        status: decode_status(row.get::<_, String>("status")?.as_str())?,
        execution: from_json(row.get("execution_json")?)?,
        limits: from_json(row.get("limits_json")?)?,
        sandbox: from_json(row.get("sandbox_json")?)?,
        metadata: from_json(row.get("metadata_json")?)?,
        created_at: ts_millis_to_utc(row.get("created_at_ms")?),
        updated_at: ts_millis_to_utc(row.get("updated_at_ms")?),
        started_at: row
            .get::<_, Option<i64>>("started_at_ms")?
            .map(ts_millis_to_utc),
        finished_at: row
            .get::<_, Option<i64>>("finished_at_ms")?
            .map(ts_millis_to_utc),
        duration_ms: row
            .get::<_, Option<i64>>("duration_ms")?
            .map(|value| value.max(0) as u64),
        shim_pid: row
            .get::<_, Option<i64>>("shim_pid")?
            .map(|value| value as u32),
        pid: row.get::<_, Option<i64>>("pid")?.map(|value| value as u32),
        pgid: row.get("pgid")?,
        exit_code: row.get("exit_code")?,
        exit_signal: row.get("exit_signal")?,
        error_code: row
            .get::<_, Option<String>>("error_code")?
            .map(|value| decode_error_code(value.as_str()))
            .transpose()?,
        error: row
            .get::<_, Option<String>>("error_json")?
            .map(from_json)
            .transpose()?,
        usage: row
            .get::<_, Option<String>>("usage_json")?
            .map(from_json)
            .transpose()?,
        task_dir: PathBuf::from(row.get::<_, String>("task_dir")?),
        workspace_dir: PathBuf::from(row.get::<_, String>("workspace_dir")?),
        request_path: PathBuf::from(row.get::<_, String>("request_path")?),
        result_path: PathBuf::from(row.get::<_, String>("result_path")?),
        stdout_path: PathBuf::from(row.get::<_, String>("stdout_path")?),
        stderr_path: PathBuf::from(row.get::<_, String>("stderr_path")?),
        script_path: row
            .get::<_, Option<String>>("script_path")?
            .map(PathBuf::from),
        stdout_max_bytes: row.get::<_, i64>("stdout_max_bytes")?.max(0) as u64,
        stderr_max_bytes: row.get::<_, i64>("stderr_max_bytes")?.max(0) as u64,
        kill_requested: row.get::<_, i64>("kill_requested")? != 0,
        kill_requested_at: row
            .get::<_, Option<i64>>("kill_requested_at_ms")?
            .map(ts_millis_to_utc),
        timeout_triggered: row.get::<_, i64>("timeout_triggered")? != 0,
        result_json: row
            .get::<_, Option<String>>("result_json")?
            .map(from_json)
            .transpose()?,
    })
}

fn insert_event_tx(
    tx: &rusqlite::Transaction<'_>,
    task_id: &str,
    event_type: EventType,
    message: Option<&str>,
    data: Option<&Value>,
) -> AppResult<()> {
    tx.execute(
        "INSERT INTO task_events (task_id, event_type, timestamp_ms, message, data_json) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            task_id,
            encode_event_type(event_type),
            Utc::now().timestamp_millis(),
            message,
            data.map(to_json).transpose()?,
        ],
    )?;
    Ok(())
}

fn to_json<T: Serialize>(value: &T) -> AppResult<String> {
    Ok(serde_json::to_string(value)?)
}

fn from_json<T: DeserializeOwned>(raw: String) -> rusqlite::Result<T> {
    serde_json::from_str(&raw).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

fn opt_json_value(raw: Option<String>) -> rusqlite::Result<Option<Value>> {
    raw.map(from_json).transpose()
}

fn encode_status(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Accepted => "accepted",
        TaskStatus::Running => "running",
        TaskStatus::Success => "success",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn decode_status(value: &str) -> rusqlite::Result<TaskStatus> {
    match value {
        "accepted" => Ok(TaskStatus::Accepted),
        "running" => Ok(TaskStatus::Running),
        "success" => Ok(TaskStatus::Success),
        "failed" => Ok(TaskStatus::Failed),
        "cancelled" => Ok(TaskStatus::Cancelled),
        other => Err(rusqlite::Error::InvalidColumnType(
            0,
            other.into(),
            rusqlite::types::Type::Text,
        )),
    }
}

fn encode_error_code(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::InvalidInput => "invalid_input",
        ErrorCode::LaunchFailed => "launch_failed",
        ErrorCode::Timeout => "timeout",
        ErrorCode::Cancelled => "cancelled",
        ErrorCode::MemoryLimitExceeded => "memory_limit_exceeded",
        ErrorCode::CpuLimitExceeded => "cpu_limit_exceeded",
        ErrorCode::ResourceLimitExceeded => "resource_limit_exceeded",
        ErrorCode::SandboxSetupFailed => "sandbox_setup_failed",
        ErrorCode::ExitNonZero => "exit_nonzero",
        ErrorCode::Internal => "internal",
    }
}

fn decode_error_code(value: &str) -> rusqlite::Result<ErrorCode> {
    match value {
        "invalid_input" => Ok(ErrorCode::InvalidInput),
        "launch_failed" => Ok(ErrorCode::LaunchFailed),
        "timeout" => Ok(ErrorCode::Timeout),
        "cancelled" => Ok(ErrorCode::Cancelled),
        "memory_limit_exceeded" => Ok(ErrorCode::MemoryLimitExceeded),
        "cpu_limit_exceeded" => Ok(ErrorCode::CpuLimitExceeded),
        "resource_limit_exceeded" => Ok(ErrorCode::ResourceLimitExceeded),
        "sandbox_setup_failed" => Ok(ErrorCode::SandboxSetupFailed),
        "exit_nonzero" => Ok(ErrorCode::ExitNonZero),
        "internal" => Ok(ErrorCode::Internal),
        other => Err(rusqlite::Error::InvalidColumnType(
            0,
            other.into(),
            rusqlite::types::Type::Text,
        )),
    }
}

fn encode_event_type(event_type: EventType) -> &'static str {
    match event_type {
        EventType::Submitted => "submitted",
        EventType::Accepted => "accepted",
        EventType::Started => "started",
        EventType::KillRequested => "kill_requested",
        EventType::TimeoutTriggered => "timeout_triggered",
        EventType::Finished => "finished",
        EventType::Failed => "failed",
        EventType::Cancelled => "cancelled",
        EventType::Recovered => "recovered",
    }
}

fn decode_event_type(value: &str) -> rusqlite::Result<EventType> {
    match value {
        "submitted" => Ok(EventType::Submitted),
        "accepted" => Ok(EventType::Accepted),
        "started" => Ok(EventType::Started),
        "kill_requested" => Ok(EventType::KillRequested),
        "timeout_triggered" => Ok(EventType::TimeoutTriggered),
        "finished" => Ok(EventType::Finished),
        "failed" => Ok(EventType::Failed),
        "cancelled" => Ok(EventType::Cancelled),
        "recovered" => Ok(EventType::Recovered),
        other => Err(rusqlite::Error::InvalidColumnType(
            0,
            other.into(),
            rusqlite::types::Type::Text,
        )),
    }
}

fn ts_millis_to_utc(value: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(value)
        .single()
        .unwrap_or_else(Utc::now)
}
