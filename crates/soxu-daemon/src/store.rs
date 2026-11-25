//! 基于 SQLite WAL 模式的守护进程本地持久化存储与单调游标事件日志。

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use soxu_core::{DraftSaveOutcome, sort_quick_chats_by_attention, sort_workspaces_by_attention};
use soxu_protocol::{
    AgentCapabilityDescriptor, ApprovalItemSnapshot, DaemonEvent, DraftSnapshot, EventEnvelope,
    ModelDescriptor, PROTOCOL_VERSION, PairedDeviceSummary, ProjectSummary, QuickChatSummary,
    SessionSummary, SoxuPreferences, StateSnapshot, TurnMessageSnapshot, WorkspaceSummary,
};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

/// 线程安全的 SQLite 本地持久化层。
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// 打开指定路径的持久化数据库文件（自动创建父目录并开启 WAL）。
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建数据库目录失败: {}", parent.display()))?;
        }
        let conn = Connection::open(db_path)
            .with_context(|| format!("打开 SQLite 数据库失败: {}", db_path.display()))?;
        Self::init_with_conn(conn)
    }

    /// 打开内存数据库（用于隔离单元与集成测试）。
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("打开内存 SQLite 失败")?;
        Self::init_with_conn(conn)
    }

    fn init_with_conn(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                updated_at TEXT NOT NULL,
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS quick_chats (
                id TEXT PRIMARY KEY,
                updated_at TEXT NOT NULL,
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                target_kind TEXT NOT NULL,
                target_id TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                turn_index INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS drafts (
                session_id TEXT PRIMARY KEY,
                generation INTEGER NOT NULL,
                updated_at TEXT NOT NULL,
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS approvals (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                resolved INTEGER NOT NULL,
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS preferences (
                singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS paired_devices (
                device_id TEXT PRIMARY KEY,
                token_sha256 TEXT NOT NULL UNIQUE,
                last_seen_at TEXT NOT NULL,
                revoked INTEGER NOT NULL,
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS event_log (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                protocol_version INTEGER NOT NULL,
                timestamp TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            ",
        )
        .context("初始化 SQLite 表结构失败")?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("SQLite 连接锁已被污染"))
    }

    /// 读取或初始化并持久化当前守护进程的稳定指纹。
    pub fn get_or_create_daemon_fingerprint(&self) -> Result<String> {
        let conn = self.lock_conn()?;
        if let Some(existing) = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'daemon_fingerprint'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(existing);
        }

        let created = format!("soxu-fp-{}", Uuid::new_v4().simple());
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('daemon_fingerprint', ?1)",
            params![created],
        )?;
        Ok(created)
    }

    pub fn upsert_project(&self, project: &ProjectSummary) -> Result<()> {
        let conn = self.lock_conn()?;
        let json = serde_json::to_string(project)?;
        conn.execute(
            "INSERT INTO projects (id, updated_at, json) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at, json = excluded.json",
            params![project.project_id, Utc::now().to_rfc3339(), json],
        )?;
        Ok(())
    }

    pub fn upsert_workspace(&self, workspace: &WorkspaceSummary) -> Result<()> {
        let conn = self.lock_conn()?;
        let json = serde_json::to_string(workspace)?;
        conn.execute(
            "INSERT INTO workspaces (id, project_id, updated_at, json) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET project_id = excluded.project_id, updated_at = excluded.updated_at, json = excluded.json",
            params![
                workspace.workspace_id,
                workspace.project_id,
                workspace.updated_at.to_rfc3339(),
                json
            ],
        )?;
        Ok(())
    }

    pub fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceSummary>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT json FROM workspaces WHERE id = ?1",
                params![workspace_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn upsert_quick_chat(&self, chat: &QuickChatSummary) -> Result<()> {
        let conn = self.lock_conn()?;
        let json = serde_json::to_string(chat)?;
        conn.execute(
            "INSERT INTO quick_chats (id, updated_at, json) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at, json = excluded.json",
            params![chat.chat_id, chat.updated_at.to_rfc3339(), json],
        )?;
        Ok(())
    }

    pub fn get_quick_chat(&self, chat_id: &str) -> Result<Option<QuickChatSummary>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT json FROM quick_chats WHERE id = ?1",
                params![chat_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn upsert_session(&self, session: &SessionSummary) -> Result<()> {
        let conn = self.lock_conn()?;
        let (target_kind, target_id) = match &session.target {
            soxu_protocol::SessionTarget::Workspace { workspace_id } => {
                ("workspace", workspace_id.as_str())
            }
            soxu_protocol::SessionTarget::QuickChat { chat_id } => ("quick_chat", chat_id.as_str()),
        };
        let json = serde_json::to_string(session)?;
        conn.execute(
            "INSERT INTO sessions (id, target_kind, target_id, updated_at, json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               target_kind = excluded.target_kind,
               target_id = excluded.target_id,
               updated_at = excluded.updated_at,
               json = excluded.json",
            params![
                session.session_id,
                target_kind,
                target_id,
                session.updated_at.to_rfc3339(),
                json
            ],
        )?;
        Ok(())
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT json FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn upsert_message(&self, msg: &TurnMessageSnapshot) -> Result<()> {
        let conn = self.lock_conn()?;
        let json = serde_json::to_string(msg)?;
        conn.execute(
            "INSERT INTO messages (id, session_id, turn_index, created_at, json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               turn_index = excluded.turn_index,
               json = excluded.json",
            params![
                msg.message_id,
                msg.session_id,
                i64::from(msg.turn_index),
                msg.created_at.to_rfc3339(),
                json
            ],
        )?;
        Ok(())
    }

    pub fn list_session_messages(&self, session_id: &str) -> Result<Vec<TurnMessageSnapshot>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT json FROM messages WHERE session_id = ?1 ORDER BY turn_index ASC, created_at ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for item in rows {
            out.push(serde_json::from_str(&item?)?);
        }
        Ok(out)
    }

    pub fn next_turn_index(&self, session_id: &str) -> Result<u32> {
        let conn = self.lock_conn()?;
        let max_turn: Option<i64> = conn.query_row(
            "SELECT MAX(turn_index) FROM messages WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(max_turn.map_or(1, |v| u32::try_from(v.saturating_add(1)).unwrap_or(1)))
    }

    /// 持久化校验并写入草稿，防止过期代次覆盖磁盘记录。
    pub fn save_draft_if_newer(&self, draft: &DraftSnapshot) -> Result<DraftSaveOutcome> {
        let conn = self.lock_conn()?;
        let existing_gen: Option<i64> = conn
            .query_row(
                "SELECT generation FROM drafts WHERE session_id = ?1",
                params![draft.session_id],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(prev) = existing_gen {
            let prev_u64 = u64::try_from(prev).unwrap_or(0);
            if draft.generation <= prev_u64 {
                return Ok(DraftSaveOutcome::RejectedStale {
                    current_generation: prev_u64,
                });
            }
        }

        let json = serde_json::to_string(draft)?;
        let gen_i64 = i64::try_from(draft.generation).unwrap_or(i64::MAX);
        conn.execute(
            "INSERT INTO drafts (session_id, generation, updated_at, json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(session_id) DO UPDATE SET
               generation = excluded.generation,
               updated_at = excluded.updated_at,
               json = excluded.json",
            params![
                draft.session_id,
                gen_i64,
                draft.updated_at.to_rfc3339(),
                json
            ],
        )?;
        Ok(DraftSaveOutcome::Accepted(draft.clone()))
    }

    pub fn get_draft(&self, session_id: &str) -> Result<Option<DraftSnapshot>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT json FROM drafts WHERE session_id = ?1",
                params![session_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn upsert_approval(&self, approval: &ApprovalItemSnapshot) -> Result<()> {
        let conn = self.lock_conn()?;
        let json = serde_json::to_string(approval)?;
        let resolved = i32::from(approval.audit.is_some());
        conn.execute(
            "INSERT INTO approvals (id, session_id, created_at, resolved, json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               resolved = excluded.resolved,
               json = excluded.json",
            params![
                approval.approval_id,
                approval.session_id,
                approval.created_at.to_rfc3339(),
                resolved,
                json
            ],
        )?;
        Ok(())
    }

    pub fn get_approval(&self, approval_id: &str) -> Result<Option<ApprovalItemSnapshot>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT json FROM approvals WHERE id = ?1",
                params![approval_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn save_preferences(&self, prefs: &SoxuPreferences) -> Result<()> {
        let conn = self.lock_conn()?;
        let json = serde_json::to_string(prefs)?;
        conn.execute(
            "INSERT INTO preferences (singleton_id, json) VALUES (1, ?1)
             ON CONFLICT(singleton_id) DO UPDATE SET json = excluded.json",
            params![json],
        )?;
        Ok(())
    }

    pub fn load_preferences(&self) -> Result<SoxuPreferences> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT json FROM preferences WHERE singleton_id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match raw {
            Some(s) => Ok(serde_json::from_str(&s)?),
            None => Ok(SoxuPreferences::default()),
        }
    }

    pub fn save_paired_device(
        &self,
        device: &PairedDeviceSummary,
        token_sha256: &str,
    ) -> Result<()> {
        let conn = self.lock_conn()?;
        let json = serde_json::to_string(device)?;
        let revoked = i32::from(device.revoked);
        conn.execute(
            "INSERT INTO paired_devices (device_id, token_sha256, last_seen_at, revoked, json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(device_id) DO UPDATE SET
               token_sha256 = excluded.token_sha256,
               last_seen_at = excluded.last_seen_at,
               revoked = excluded.revoked,
               json = excluded.json",
            params![
                device.device_id,
                token_sha256,
                device.last_seen_at.to_rfc3339(),
                revoked,
                json
            ],
        )?;
        Ok(())
    }

    pub fn find_device_by_token_hash(
        &self,
        token_sha256: &str,
    ) -> Result<Option<PairedDeviceSummary>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT json FROM paired_devices WHERE token_sha256 = ?1",
                params![token_sha256],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn get_device(&self, device_id: &str) -> Result<Option<PairedDeviceSummary>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT json FROM paired_devices WHERE device_id = ?1",
                params![device_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    pub fn touch_device_seen(
        &self,
        device_id: &str,
        seen_at: DateTime<Utc>,
        online: bool,
    ) -> Result<Option<PairedDeviceSummary>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT json FROM paired_devices WHERE device_id = ?1",
                params![device_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(s) = raw else {
            return Ok(None);
        };
        let mut device: PairedDeviceSummary = serde_json::from_str(&s)?;
        device.last_seen_at = seen_at;
        device.online = online;
        let updated_json = serde_json::to_string(&device)?;
        conn.execute(
            "UPDATE paired_devices SET last_seen_at = ?2, json = ?3 WHERE device_id = ?1",
            params![device_id, seen_at.to_rfc3339(), updated_json],
        )?;
        Ok(Some(device))
    }

    pub fn revoke_device(&self, device_id: &str) -> Result<Option<PairedDeviceSummary>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT json FROM paired_devices WHERE device_id = ?1",
                params![device_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(s) = raw else {
            return Ok(None);
        };
        let mut device: PairedDeviceSummary = serde_json::from_str(&s)?;
        device.revoked = true;
        device.online = false;
        let updated_json = serde_json::to_string(&device)?;
        conn.execute(
            "UPDATE paired_devices SET revoked = 1, json = ?2 WHERE device_id = ?1",
            params![device_id, updated_json],
        )?;
        Ok(Some(device))
    }

    /// 原子追加事件日志并分配严格单调递增 `seq`。
    pub fn append_event(&self, event: &DaemonEvent) -> Result<EventEnvelope<DaemonEvent>> {
        let conn = self.lock_conn()?;
        let timestamp = Utc::now();
        let payload_json = serde_json::to_string(event)?;
        conn.execute(
            "INSERT INTO event_log (protocol_version, timestamp, payload_json) VALUES (?1, ?2, ?3)",
            params![
                i64::from(PROTOCOL_VERSION),
                timestamp.to_rfc3339(),
                payload_json
            ],
        )?;
        let seq = u64::try_from(conn.last_insert_rowid()).unwrap_or(0);
        Ok(EventEnvelope {
            protocol_version: PROTOCOL_VERSION,
            seq,
            timestamp,
            payload: event.clone(),
        })
    }

    pub fn latest_seq(&self) -> Result<u64> {
        let conn = self.lock_conn()?;
        let max_seq: Option<i64> =
            conn.query_row("SELECT MAX(seq) FROM event_log", [], |row| row.get(0))?;
        Ok(max_seq.map_or(0, |v| u64::try_from(v).unwrap_or(0)))
    }

    pub fn query_events_after(
        &self,
        after_seq: u64,
        limit: u32,
    ) -> Result<Vec<EventEnvelope<DaemonEvent>>> {
        let conn = self.lock_conn()?;
        let after_i64 = i64::try_from(after_seq).unwrap_or(i64::MAX);
        let limit_i64 = i64::from(limit.clamp(1, 2000));
        let mut stmt = conn.prepare(
            "SELECT seq, protocol_version, timestamp, payload_json
             FROM event_log
             WHERE seq > ?1
             ORDER BY seq ASC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![after_i64, limit_i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for item in rows {
            let (seq_i64, proto_i64, ts_str, payload_json) = item?;
            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let payload: DaemonEvent = serde_json::from_str(&payload_json)?;
            out.push(EventEnvelope {
                protocol_version: u32::try_from(proto_i64).unwrap_or(PROTOCOL_VERSION),
                seq: u64::try_from(seq_i64).unwrap_or(0),
                timestamp,
                payload,
            });
        }
        Ok(out)
    }

    /// 组装无阻塞全量内存快照供桌面与移动客户端首屏渲染。
    pub fn load_snapshot(
        &self,
        agents: &[AgentCapabilityDescriptor],
        models: &[ModelDescriptor],
        subagents: Vec<soxu_protocol::SubagentSummary>,
        pending_inputs: Vec<soxu_protocol::PendingUserInputRequest>,
    ) -> Result<StateSnapshot> {
        let preferences = self.load_preferences()?;
        let conn = self.lock_conn()?;

        let projects = load_table_json::<ProjectSummary>(
            &conn,
            "SELECT json FROM projects ORDER BY updated_at DESC",
        )?;
        let mut workspaces = load_table_json::<WorkspaceSummary>(
            &conn,
            "SELECT json FROM workspaces ORDER BY updated_at DESC",
        )?;
        sort_workspaces_by_attention(&mut workspaces);

        let mut quick_chats = load_table_json::<QuickChatSummary>(
            &conn,
            "SELECT json FROM quick_chats ORDER BY updated_at DESC",
        )?;
        sort_quick_chats_by_attention(&mut quick_chats);

        let sessions = load_table_json::<SessionSummary>(
            &conn,
            "SELECT json FROM sessions ORDER BY updated_at DESC",
        )?;
        let pending_approvals = load_table_json::<ApprovalItemSnapshot>(
            &conn,
            "SELECT json FROM approvals WHERE resolved = 0 ORDER BY created_at ASC",
        )?;
        let paired_devices = load_table_json::<PairedDeviceSummary>(
            &conn,
            "SELECT json FROM paired_devices ORDER BY last_seen_at DESC",
        )?;

        Ok(StateSnapshot {
            projects,
            workspaces,
            quick_chats,
            sessions,
            pending_approvals,
            pending_inputs,
            subagents,
            agents: agents.to_vec(),
            models: models.to_vec(),
            preferences,
            paired_devices,
        })
    }
}

fn load_table_json<T: serde::de::DeserializeOwned>(conn: &Connection, sql: &str) -> Result<Vec<T>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(serde_json::from_str(&row?)?);
    }
    Ok(out)
}
