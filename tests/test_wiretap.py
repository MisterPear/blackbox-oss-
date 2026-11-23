import tempfile
import unittest
from pathlib import Path
from memoria.wiretap import run_command_wiretap
from memoria.ledger import MemoriaLedger
from memoria.viewer import generate_html_report

class TestMemoriaWiretap(unittest.TestCase):
    def test_command_wiretap_and_export(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = Path(tmpdir) / "test_wiretap.db"
            html_out = Path(tmpdir) / "report.html"

            code = run_command_wiretap(
                ["echo", "wiretap_verified"],
                project_name="unit_test",
                session_id="test_sess",
                db_path=db_path
            )
            self.assertEqual(code, 0)

            ledger = MemoriaLedger(db_path=db_path)
            actions = ledger.get_recent_actions("unit_test")
            self.assertEqual(len(actions), 1)
            self.assertEqual(actions[0]["target"], "echo")
            self.assertIn("wiretap_verified", actions[0]["result_snippet"])

            valid, count, err = ledger.verify_integrity("unit_test")
            self.assertTrue(valid)
            self.assertEqual(count, 1)

            out_file = generate_html_report("unit_test", db_path=db_path, output_path=html_out)
            self.assertTrue(out_file.exists())
            content = out_file.read_text()
            self.assertIn("wiretap_verified", content)
            self.assertIn("HASH CHAIN VALID", content)

if __name__ == "__main__":
    unittest.main()
