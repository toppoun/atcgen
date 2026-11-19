"""Small tests of the byte/diagnostic comparator, without a corpus run."""
import gzip
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import phase8b_regression as regression


class RegressionTests(unittest.TestCase):
    def test_diagnostics_strip_only_filename_and_preserve_line_column_message(self):
        left = regression.diagnostic(b"C:\\temporary\\case.gen:12:7: error in expression `1/0`\n")
        right = regression.diagnostic(b"/another/case.gen:12:7: error in expression `1/0`\n")
        self.assertEqual(left, right)
        for raw in (b"case.gen:13:7: error in expression `1/0`\n",
                    b"case.gen:12:8: error in expression `1/0`\n",
                    b"case.gen:12:7: another error\n"):
            self.assertNotEqual(left, regression.diagnostic(raw))

    def setup_manifest(self, root, status):
        source = "generate {\n}\ninput {\n}\n"
        sha = regression.digest(source.encode())
        output = b"1\n" if status == "passed" else b""
        stderr = b"" if status == "passed" else b"old.gen:2:3: output token limit exceeded\n"
        saved = {"problem": "example/a", "profile": "fixed", "source_sha256": sha, "seed": 42,
                 "status": status, "exit_code": 0 if status == "passed" else 1,
                 "stderr": stderr.decode(), "stdout_sha256": regression.digest(output),
                 "output_file": "case.gz"}
        if status == "limited":
            saved["diagnostic"] = regression.diagnostic(stderr)
        manifest = {"baseline_commit": "reference", "historical_report_sha256": regression.digest(regression.HISTORICAL.read_bytes()),
                    "sources": {sha: source}, "conditions": [saved], "unsupported": []}
        path = root / "manifest.json"
        path.write_text(json.dumps(manifest))
        (root / "case.gz").write_bytes(gzip.compress(output))
        return path, saved

    def test_compares_raw_bytes_and_rejects_same_tokens_with_different_whitespace(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path, _ = self.setup_manifest(root, "passed")
            checker = lambda case: case.row(1)
            with patch.dict(regression.v.CHECKERS, {("example", "a"): checker}):
                with patch.object(regression, "run", return_value=subprocess.CompletedProcess([], 0, b"1\n", b"")):
                    self.assertEqual(regression.compare(path, root, Path("binary"))["successful_byte_matches"], 1)
                with patch.object(regression, "run", return_value=subprocess.CompletedProcess([], 0, b"1 \n", b"")):
                    with self.assertRaisesRegex(ValueError, "raw stdout changed"):
                        regression.compare(path, root, Path("binary"))

    def test_known_limit_success_or_partial_stdout_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path, _ = self.setup_manifest(root, "limited")
            good = subprocess.CompletedProcess([], 1, b"", b"new.gen:2:3: output token limit exceeded\n")
            with patch.object(regression, "run", return_value=good):
                self.assertEqual(regression.compare(path, root, Path("binary"))["known_limited_diagnostic_matches"], 1)
            for result in (subprocess.CompletedProcess([], 0, b"", b""),
                           subprocess.CompletedProcess([], 1, b"partial", good.stderr)):
                with patch.object(regression, "run", return_value=result):
                    with self.assertRaises(ValueError):
                        regression.compare(path, root, Path("binary"))

    def test_damaged_baseline_is_rejected_before_cli_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path, _ = self.setup_manifest(root, "passed")
            (root / "case.gz").write_bytes(gzip.compress(b"changed\n"))
            with patch.object(regression, "run") as run:
                with self.assertRaisesRegex(ValueError, "baseline bytes damaged"):
                    regression.compare(path, root, Path("binary"))
                run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
