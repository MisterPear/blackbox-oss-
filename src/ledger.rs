use chrono::Utc;
use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub fn get_iso_timestamp() -> String {
    Utc::now().to_rfc3339()
}

pub fn safe_truncate_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAction {
    pub id: i64,
    pub project_name: String,
    pub session_id: String,
    pub timestamp_start: String,
    pub timestamp_end: String,
    pub duration_ms: f64,
    pub action_type: String,
    pub target: String,
    pub parameters: Value,
    pub result_snippet: Option<String>,
    pub exit_code: i32,
    pub bytes_out: i64,
    pub prev_hash: String,
    pub entry_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageContext {
    pub id: i64,
    pub project_name: String,
    pub session_id: String,
    pub stage_name: String,
    pub step_description: String,
    pub outcome: String,
    pub first_logged: String,
    pub last_updated: String,
    pub action_ids: Vec<i64>,
    pub notes: Option<String>,
    pub actor: String,
}

pub struct MemoriaLedger {
    pub db_path: PathBuf,
}

impl MemoriaLedger {
    pub fn new(db_path: Option<PathBuf>) -> SqlResult<Self> {
        let path = match db_path {
            Some(p) => p,
            None => {
                if let Ok(env_path) = std::env::var("MEMORIA_DB_PATH") {
                    PathBuf::from(env_path)
                } else {
                    PathBuf::from(".memoria").join("memoria.db")
                }
            }
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let ledger = Self { db_path: path };
        ledger.init_db()?;
        Ok(ledger)
    }

    pub fn get_connection(&self) -> SqlResult<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout=5000;",
        )?;
        Ok(conn)
    }

    fn init_db(&self) -> SqlResult<()> {
        let conn = self.get_connection()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS task_ledger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_name TEXT NOT NULL,
                session_id TEXT NOT NULL,
                timestamp_start TEXT NOT NULL,
                timestamp_end TEXT NOT NULL,
                duration_ms REAL NOT NULL,
                action_type TEXT NOT NULL,
                target TEXT NOT NULL,
                parameters TEXT NOT NULL,
                result_snippet TEXT,
                exit_code INTEGER NOT NULL,
                bytes_out INTEGER DEFAULT 0,
                prev_hash TEXT NOT NULL,
                entry_hash TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS stage_ledger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_name TEXT NOT NULL,
                session_id TEXT NOT NULL,
                stage_name TEXT NOT NULL,
                step_description TEXT NOT NULL,
                outcome TEXT NOT NULL,
                first_logged TEXT NOT NULL,
                last_updated TEXT NOT NULL,
                action_ids TEXT NOT NULL,
                notes TEXT,
                actor TEXT NOT NULL DEFAULT 'agent'
            );
            CREATE INDEX IF NOT EXISTS idx_tl_project ON task_ledger(project_name);
            CREATE INDEX IF NOT EXISTS idx_tl_session ON task_ledger(session_id);
            CREATE INDEX IF NOT EXISTS idx_sl_project ON stage_ledger(project_name);

            -- DB-Level Immutability: Prevent row modification or deletion
            CREATE TRIGGER IF NOT EXISTS prevent_task_ledger_update
            BEFORE UPDATE ON task_ledger
            BEGIN
                SELECT RAISE(ABORT, 'MEMORIA_SECURITY_VIOLATION: task_ledger is strictly append-only. Modifying existing entries is prohibited.');
            END;

            CREATE TRIGGER IF NOT EXISTS prevent_task_ledger_delete
            BEFORE DELETE ON task_ledger
            BEGIN
                SELECT RAISE(ABORT, 'MEMORIA_SECURITY_VIOLATION: task_ledger is strictly append-only. Deleting ledger entries is prohibited.');
            END;

            CREATE TRIGGER IF NOT EXISTS prevent_stage_ledger_update
            BEFORE UPDATE ON stage_ledger
            BEGIN
                SELECT RAISE(ABORT, 'MEMORIA_SECURITY_VIOLATION: stage_ledger is strictly append-only. Modifying existing stages is prohibited.');
            END;

            CREATE TRIGGER IF NOT EXISTS prevent_stage_ledger_delete
            BEFORE DELETE ON stage_ledger
            BEGIN
                SELECT RAISE(ABORT, 'MEMORIA_SECURITY_VIOLATION: stage_ledger is strictly append-only. Deleting stage entries is prohibited.');
            END;",
        )?;

        // Ensure actor column exists in stage_ledger for backward-compatible schemas
        let _ = conn.execute("ALTER TABLE stage_ledger ADD COLUMN actor TEXT NOT NULL DEFAULT 'agent'", []);

        Ok(())
    }

    pub fn calculate_entry_hash(prev_hash: &str, payload: &Value) -> String {
        let canonical_json = serde_json::to_string(payload).unwrap_or_default();
        let digest_input = format!("{}:{}", prev_hash, canonical_json);
        let mut hasher = Sha256::new();
        hasher.update(digest_input.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn record_action(
        &self,
        project_name: &str,
        session_id: &str,
        action_type: &str,
        target: &str,
        parameters: Value,
        start_time: &str,
        end_time: &str,
        duration_ms: f64,
        exit_code: i32,
        result_snippet: Option<&str>,
        bytes_out: i64,
    ) -> SqlResult<i64> {
        let mut conn = self.get_connection()?;
        // Concurrency Safety: Immediate transaction acquires write lock upfront, preventing hash chain forks
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        
        let prev_hash: String = tx
            .query_row(
                "SELECT entry_hash FROM task_ledger WHERE project_name=?1 ORDER BY id DESC LIMIT 1",
                params![project_name],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| GENESIS_HASH.to_string());

        let params_json = serde_json::to_string(&parameters).unwrap_or_else(|_| "{}".to_string());
        
        // Build sorted JSON payload for canonical hashing matching Python reference
        let rounded_duration = (duration_ms * 1000.0).round() / 1000.0;
        let mut hash_map = serde_json::Map::new();
        hash_map.insert("action_type".to_string(), Value::String(action_type.to_string()));
        hash_map.insert("bytes_out".to_string(), Value::Number(bytes_out.into()));
        hash_map.insert("duration_ms".to_string(), serde_json::Number::from_f64(rounded_duration).map(Value::Number).unwrap_or(Value::Null));
        hash_map.insert("exit_code".to_string(), Value::Number(exit_code.into()));
        hash_map.insert("parameters".to_string(), Value::String(params_json.clone()));
        hash_map.insert("project_name".to_string(), Value::String(project_name.to_string()));
        hash_map.insert("session_id".to_string(), Value::String(session_id.to_string()));
        hash_map.insert("target".to_string(), Value::String(target.to_string()));
        hash_map.insert("timestamp_end".to_string(), Value::String(end_time.to_string()));
        hash_map.insert("timestamp_start".to_string(), Value::String(start_time.to_string()));
        let hashable_payload = Value::Object(hash_map);

        let entry_hash = Self::calculate_entry_hash(&prev_hash, &hashable_payload);

        // UTF-8 Hardening: Safe character boundary truncation to prevent multi-byte panic
        let snippet_truncated = result_snippet.map(|s| safe_truncate_str(s, 2000));

        tx.execute(
            "INSERT INTO task_ledger (
                project_name, session_id, timestamp_start, timestamp_end,
                duration_ms, action_type, target, parameters,
                result_snippet, exit_code, bytes_out, prev_hash, entry_hash
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                project_name,
                session_id,
                start_time,
                end_time,
                duration_ms,
                action_type,
                target,
                params_json,
                snippet_truncated,
                exit_code,
                bytes_out,
                prev_hash,
                entry_hash
            ],
        )?;

        let row_id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(row_id)
    }

    pub fn commit_stage(
        &self,
        project_name: &str,
        session_id: &str,
        stage_name: &str,
        step_description: &str,
        outcome: &str,
        action_ids: &[i64],
        notes: Option<&str>,
        actor: &str,
    ) -> SqlResult<i64> {
        let mut conn = self.get_connection()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let now = get_iso_timestamp();
        let ids_json = serde_json::to_string(action_ids).unwrap_or_else(|_| "[]".to_string());

        tx.execute(
            "INSERT INTO stage_ledger (
                project_name, session_id, stage_name, step_description,
                outcome, first_logged, last_updated, action_ids, notes, actor
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                project_name,
                session_id,
                stage_name,
                step_description,
                outcome,
                now,
                now,
                ids_json,
                notes,
                actor
            ],
        )?;

        let row_id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(row_id)
    }

    pub fn get_recent_actions(&self, project_name: &str, limit: usize) -> SqlResult<Vec<TaskAction>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, project_name, session_id, timestamp_start, timestamp_end,
                    duration_ms, action_type, target, parameters, result_snippet,
                    exit_code, bytes_out, prev_hash, entry_hash
             FROM task_ledger
             WHERE project_name=?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![project_name, limit as i64], |row| {
            let params_str: String = row.get(8)?;
            let params_val: Value = serde_json::from_str(&params_str).unwrap_or(Value::Null);

            Ok(TaskAction {
                id: row.get(0)?,
                project_name: row.get(1)?,
                session_id: row.get(2)?,
                timestamp_start: row.get(3)?,
                timestamp_end: row.get(4)?,
                duration_ms: row.get(5)?,
                action_type: row.get(6)?,
                target: row.get(7)?,
                parameters: params_val,
                result_snippet: row.get(9)?,
                exit_code: row.get(10)?,
                bytes_out: row.get(11)?,
                prev_hash: row.get(12)?,
                entry_hash: row.get(13)?,
            })
        })?;

        let mut actions = Vec::new();
        for r in rows {
            actions.push(r?);
        }
        Ok(actions)
    }

    pub fn get_context(&self, project_name: &str, limit: usize) -> SqlResult<Vec<StageContext>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, project_name, session_id, stage_name, step_description,
                    outcome, first_logged, last_updated, action_ids, notes, actor
             FROM stage_ledger
             WHERE project_name=?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![project_name, limit as i64], |row| {
            let ids_str: String = row.get(8)?;
            let ids_val: Vec<i64> = serde_json::from_str(&ids_str).unwrap_or_default();

            Ok(StageContext {
                id: row.get(0)?,
                project_name: row.get(1)?,
                session_id: row.get(2)?,
                stage_name: row.get(3)?,
                step_description: row.get(4)?,
                outcome: row.get(5)?,
                first_logged: row.get(6)?,
                last_updated: row.get(7)?,
                action_ids: ids_val,
                notes: row.get(9)?,
                actor: row.get::<_, Option<String>>(10)?.unwrap_or_else(|| "agent".to_string()),
            })
        })?;

        let mut stages = Vec::new();
        for r in rows {
            stages.push(r?);
        }
        Ok(stages)
    }

    pub fn verify_integrity(&self, project_name: &str) -> SqlResult<(bool, usize, Option<String>)> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, project_name, session_id, timestamp_start, timestamp_end,
                    duration_ms, action_type, target, parameters, exit_code,
                    bytes_out, prev_hash, entry_hash
             FROM task_ledger
             WHERE project_name=?1
             ORDER BY id ASC",
        )?;

        let rows = stmt.query_map(params![project_name], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i32>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
            ))
        })?;

        let mut expected_prev = GENESIS_HASH.to_string();
        let mut count = 0;

        for r_res in rows {
            let (
                id,
                p_name,
                s_id,
                t_start,
                t_end,
                d_ms,
                a_type,
                target,
                params_str,
                exit_code,
                bytes_out,
                prev_h,
                entry_h,
            ) = r_res?;

            if prev_h != expected_prev {
                return Ok((
                    false,
                    count,
                    Some(format!(
                        "Hash broken at block id={}: expected prev {}, got {}",
                        id, expected_prev, prev_h
                    )),
                ));
            }

            let rounded_duration = (d_ms * 1000.0).round() / 1000.0;
            let mut hash_map = serde_json::Map::new();
            hash_map.insert("action_type".to_string(), Value::String(a_type));
            hash_map.insert("bytes_out".to_string(), Value::Number(bytes_out.into()));
            hash_map.insert("duration_ms".to_string(), serde_json::Number::from_f64(rounded_duration).map(Value::Number).unwrap_or(Value::Null));
            hash_map.insert("exit_code".to_string(), Value::Number(exit_code.into()));
            hash_map.insert("parameters".to_string(), Value::String(params_str));
            hash_map.insert("project_name".to_string(), Value::String(p_name));
            hash_map.insert("session_id".to_string(), Value::String(s_id));
            hash_map.insert("target".to_string(), Value::String(target));
            hash_map.insert("timestamp_end".to_string(), Value::String(t_end));
            hash_map.insert("timestamp_start".to_string(), Value::String(t_start));
            let payload = Value::Object(hash_map);

            let computed = Self::calculate_entry_hash(&expected_prev, &payload);
            if computed != entry_h {
                return Ok((
                    false,
                    count,
                    Some(format!(
                        "Hash mismatch at block id={}: computed {}, got {}",
                        id, computed, entry_h
                    )),
                ));
            }

            expected_prev = entry_h;
            count += 1;
        }

        Ok((true, count, None))
    }
}
