import hashlib
import json
import os
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

GENESIS_HASH = "0" * 64

def get_iso_timestamp() -> str:
    """Return current UTC time in ISO format with microsecond precision."""
    return datetime.now(timezone.utc).isoformat()

class MemoriaLedger:
    """
    Self-contained, append-only cryptographic ledger for AI agent actions and stages.
    Operates strictly on a local SQLite database in WAL mode.
    """

    def __init__(self, db_path: Optional[Path] = None):
        if db_path is None:
            env_db = os.environ.get("MEMORIA_DB_PATH")
            if env_db:
                self.db_path = Path(env_db)
            else:
                self.db_path = Path.cwd() / ".memoria" / "memoria.db"
        else:
            self.db_path = Path(db_path)

        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._init_db()

    def _get_connection(self) -> sqlite3.Connection:
        con = sqlite3.connect(str(self.db_path), timeout=10.0)
        con.execute("PRAGMA journal_mode=WAL;")
        con.execute("PRAGMA synchronous=NORMAL;")
        con.execute("PRAGMA busy_timeout=5000;")
        return con

    def _init_db(self) -> None:
        with self._get_connection() as con:
            con.execute("""
                CREATE TABLE IF NOT EXISTS task_ledger (
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
                    exit_code INTEGER,
                    bytes_out INTEGER DEFAULT 0,
                    prev_hash TEXT NOT NULL,
                    entry_hash TEXT NOT NULL
                );
            """)
            con.execute("""
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
                    notes TEXT
                );
            """)
            con.execute("CREATE INDEX IF NOT EXISTS idx_tl_project ON task_ledger(project_name);")
            con.execute("CREATE INDEX IF NOT EXISTS idx_tl_session ON task_ledger(session_id);")
            con.execute("CREATE INDEX IF NOT EXISTS idx_sl_project ON stage_ledger(project_name);")

    def _calculate_entry_hash(self, prev_hash: str, payload: Dict[str, Any]) -> str:
        canonical_json = json.dumps(payload, sort_keys=True, separators=(",", ":"))
        digest_input = f"{prev_hash}:{canonical_json}".encode("utf-8")
        return hashlib.sha256(digest_input).hexdigest()

    def record_action(
        self,
        project_name: str,
        session_id: str,
        action_type: str,
        target: str,
        parameters: Dict[str, Any],
        start_time: str,
        end_time: str,
        duration_ms: float,
        exit_code: int,
        result_snippet: str = "",
        bytes_out: int = 0
    ) -> int:
        """
        Append an immutable action to task_ledger with cryptographic chaining.
        """
        with self._get_connection() as con:
            cur = con.cursor()
            cur.execute(
                "SELECT entry_hash FROM task_ledger WHERE project_name=? ORDER BY id DESC LIMIT 1",
                (project_name,)
            )
            row = cur.fetchone()
            prev_hash = row[0] if row else GENESIS_HASH

            params_json = json.dumps(parameters, sort_keys=True)
            hashable_payload = {
                "project_name": project_name,
                "session_id": session_id,
                "timestamp_start": start_time,
                "timestamp_end": end_time,
                "duration_ms": round(duration_ms, 3),
                "action_type": action_type,
                "target": target,
                "parameters": params_json,
                "exit_code": exit_code,
                "bytes_out": bytes_out
            }
            entry_hash = self._calculate_entry_hash(prev_hash, hashable_payload)

            cur.execute("""
                INSERT INTO task_ledger (
                    project_name, session_id, timestamp_start, timestamp_end,
                    duration_ms, action_type, target, parameters,
                    result_snippet, exit_code, bytes_out, prev_hash, entry_hash
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """, (
                project_name, session_id, start_time, end_time,
                duration_ms, action_type, target, params_json,
                result_snippet[:2000], exit_code, bytes_out, prev_hash, entry_hash
            ))
            con.commit()
            return cur.lastrowid

    def commit_stage(
        self,
        project_name: str,
        session_id: str,
        stage_name: str,
        step_description: str,
        outcome: str = "success",
        action_ids: Optional[List[int]] = None,
        notes: str = ""
    ) -> int:
        """
        Record or advance a project stage milestone in stage_ledger.
        """
        now = get_iso_timestamp()
        ids_json = json.dumps(action_ids or [])
        with self._get_connection() as con:
            cur = con.cursor()
            cur.execute("""
                INSERT INTO stage_ledger (
                    project_name, session_id, stage_name, step_description,
                    outcome, first_logged, last_updated, action_ids, notes
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            """, (
                project_name, session_id, stage_name, step_description,
                outcome, now, now, ids_json, notes
            ))
            con.commit()
            return cur.lastrowid

    def get_recent_actions(self, project_name: str, limit: int = 15) -> List[Dict[str, Any]]:
        """Query raw action index so agent/user can ground reality."""
        with self._get_connection() as con:
            con.row_factory = sqlite3.Row
            cur = con.execute("""
                SELECT id, session_id, timestamp_start, duration_ms, action_type,
                       target, parameters, exit_code, result_snippet, entry_hash
                FROM task_ledger
                WHERE project_name=?
                ORDER BY id DESC
                LIMIT ?
            """, (project_name, limit))
            rows = []
            for r in cur.fetchall():
                d = dict(r)
                try:
                    d["parameters"] = json.loads(d["parameters"])
                except Exception:
                    pass
                rows.append(d)
            return rows

    def get_context(self, project_name: str, limit: int = 10) -> List[Dict[str, Any]]:
        """Query project stages and milestones."""
        with self._get_connection() as con:
            con.row_factory = sqlite3.Row
            cur = con.execute("""
                SELECT id, session_id, stage_name, step_description, outcome,
                       last_updated, action_ids, notes
                FROM stage_ledger
                WHERE project_name=?
                ORDER BY id DESC
                LIMIT ?
            """, (project_name, limit))
            rows = []
            for r in cur.fetchall():
                d = dict(r)
                try:
                    d["action_ids"] = json.loads(d["action_ids"])
                except Exception:
                    pass
                rows.append(d)
            return rows

    def verify_integrity(self, project_name: str) -> Tuple[bool, int, Optional[str]]:
        """
        Walk the hash chain for a project and verify zero tampering.
        Returns (is_valid, verified_count, error_reason).
        """
        with self._get_connection() as con:
            con.row_factory = sqlite3.Row
            cur = con.execute("""
                SELECT id, project_name, session_id, timestamp_start, timestamp_end,
                       duration_ms, action_type, target, parameters, exit_code,
                       bytes_out, prev_hash, entry_hash
                FROM task_ledger
                WHERE project_name=?
                ORDER BY id ASC
            """, (project_name,))
            rows = cur.fetchall()

        if not rows:
            return True, 0, None

        expected_prev = GENESIS_HASH
        for idx, r in enumerate(rows):
            if r["prev_hash"] != expected_prev:
                return False, idx, f"Hash broken at block id={r['id']}: expected prev {expected_prev}, got {r['prev_hash']}"

            payload = {
                "project_name": r["project_name"],
                "session_id": r["session_id"],
                "timestamp_start": r["timestamp_start"],
                "timestamp_end": r["timestamp_end"],
                "duration_ms": round(r["duration_ms"], 3),
                "action_type": r["action_type"],
                "target": r["target"],
                "parameters": r["parameters"],
                "exit_code": r["exit_code"],
                "bytes_out": r["bytes_out"]
            }
            computed_hash = self._calculate_entry_hash(expected_prev, payload)
            if computed_hash != r["entry_hash"]:
                return False, idx, f"Hash mismatch at block id={r['id']}: computed {computed_hash}, got {r['entry_hash']}"

            expected_prev = r["entry_hash"]

        return True, len(rows), None
