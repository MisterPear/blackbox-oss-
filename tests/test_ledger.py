import tempfile
import unittest
from pathlib import Path
from memoria.ledger import MemoriaLedger

class TestMemoriaLedger(unittest.TestCase):
    def test_ledger_chain_and_tamper_detection(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = Path(tmpdir) / "test_memoria.db"
            ledger = MemoriaLedger(db_path=db_path)

            id1 = ledger.record_action(
                project_name="demo",
                session_id="s1",
                action_type="cli",
                target="git status",
                parameters={"cmd": "git status"},
                start_time="2026-11-23T12:00:00Z",
                end_time="2026-11-23T12:00:01Z",
                duration_ms=1000.0,
                exit_code=0,
                result_snippet="clean",
                bytes_out=5
            )
            self.assertEqual(id1, 1)

            id2 = ledger.record_action(
                project_name="demo",
                session_id="s1",
                action_type="cli",
                target="pytest",
                parameters={"cmd": "pytest"},
                start_time="2026-11-23T12:00:02Z",
                end_time="2026-11-23T12:00:04Z",
                duration_ms=2000.0,
                exit_code=0,
                result_snippet="10 passed",
                bytes_out=9
            )
            self.assertEqual(id2, 2)

            valid, count, err = ledger.verify_integrity("demo")
            self.assertTrue(valid)
            self.assertEqual(count, 2)
            self.assertIsNone(err)

            con = ledger._get_connection()
            con.execute("UPDATE task_ledger SET exit_code=1 WHERE id=1")
            con.commit()
            con.close()

            valid_tampered, count_tampered, err_tampered = ledger.verify_integrity("demo")
            self.assertFalse(valid_tampered)
            self.assertIn("Hash mismatch", err_tampered)

if __name__ == "__main__":
    unittest.main()
