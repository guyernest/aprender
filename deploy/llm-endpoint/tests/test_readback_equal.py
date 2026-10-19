"""Tests for readback_equal.py. Run: python3 -I -m unittest deploy/llm-endpoint/tests/test_readback_equal.py"""
import json
import os
import subprocess
import sys
import tempfile
import unittest

SCRIPT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "readback_equal.py")

BASE = {
    "function_name": "aprender-llm-endpoint-dev-2b",
    "state": "Active",
    "memory_mb": 10240,
    "timeout_s": 900,
    "ephemeral_mb": 10240,
    "architecture": "arm64",
    "package_type": "Image",
    "code_sha256": "abc",
    "image_uri": "123.dkr.ecr.us-east-1.amazonaws.com/r@sha256:1",
    "last_modified": "2026-10-18T10:00:00.000+0000",
    "revision_id": "rev-1",
    "env_keys": ["APR_API_KEY_HASH", "MODEL_S3_URI"],
    "reserved_concurrency": 1,
    "function_url": "absent",
    "read_at": "2026-10-18T10:00:00Z",
}


def run(*records):
    with tempfile.TemporaryDirectory() as d:
        paths = []
        for i, rec in enumerate(records):
            p = os.path.join(d, "r%d.json" % i)
            if rec is not None:
                with open(p, "w") as f:
                    json.dump(rec, f)
            paths.append(p)
        return subprocess.run([sys.executable, "-I", SCRIPT] + paths, capture_output=True, text=True)


class ReadbackEqual(unittest.TestCase):
    def test_two_equal_records_pass(self):
        other = dict(BASE, read_at="2026-10-18T11:00:00Z", env_keys=["MODEL_S3_URI", "APR_API_KEY_HASH"])
        self.assertEqual(run(BASE, other).returncode, 0)

    def test_one_changed_field_fails_and_names_it(self):
        r = run(BASE, dict(BASE, memory_mb=3008))
        self.assertEqual(r.returncode, 1)
        self.assertIn("DIFFERS memory_mb", r.stdout)

    def test_missing_field_is_an_error(self):
        broken = {k: v for k, v in BASE.items() if k != "revision_id"}
        self.assertEqual(run(BASE, broken).returncode, 2)

    def test_missing_file_is_an_error(self):
        self.assertEqual(run(BASE, None).returncode, 2)

    def test_null_field_in_both_files_is_an_error_naming_it(self):
        r = run(dict(BASE, function_name=None), dict(BASE, function_name=None))
        self.assertEqual(r.returncode, 2)
        self.assertIn("function_name", r.stderr)

    def test_mixed_type_env_keys_is_an_error(self):
        r = run(BASE, dict(BASE, env_keys=["A", 1]))
        self.assertEqual(r.returncode, 2)
        self.assertIn("env_keys", r.stderr)

    def test_null_reserved_concurrency_in_both_is_equal(self):
        self.assertEqual(run(dict(BASE, reserved_concurrency=None), dict(BASE, reserved_concurrency=None)).returncode, 0)

    def test_single_file_is_an_error(self):
        self.assertEqual(run(BASE).returncode, 2)

    def test_env_keys_content_change_differs(self):
        r = run(BASE, dict(BASE, env_keys=["APR_API_KEY_HASH", "OTHER"]))
        self.assertEqual(r.returncode, 1)
        self.assertIn("DIFFERS env_keys", r.stdout)


if __name__ == "__main__":
    unittest.main()
