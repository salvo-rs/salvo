#!/usr/bin/env python3
"""Exercise the CI audit command against an offline, synthetic advisory DB."""

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
import importlib.util
from unittest import mock


class AuditPolicyTests(unittest.TestCase):
    def test_evidence_distinguishes_findings_from_an_unfinished_audit(self):
        script = pathlib.Path(__file__).with_name("audit_lockfile.py")
        spec = importlib.util.spec_from_file_location("audit_lockfile", script)
        module = importlib.util.module_from_spec(spec)
        bytecode = sys.dont_write_bytecode
        try:
            sys.dont_write_bytecode = True
            spec.loader.exec_module(module)
        finally:
            sys.dont_write_bytecode = bytecode
        report = {"database": {"last-commit": "fixture"}, "lockfile": {"dependency-count": 1},
                  "vulnerabilities": {"found": False, "list": []},
                  "warnings": {"unsound": [{"advisory": {"id": "RUSTSEC-2020-0001"}}]}}
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            lockfile = root / "input.lock"
            lockfile.write_text("version = 4\n", encoding="utf-8")
            cases = [(json.dumps(report), 1, "findings"), ("", 2, "tool_error"), ("invalid", 0, "tool_error")]
            report["warnings"] = {}
            cases.append((json.dumps(report), 0, "clean"))
            cases.append((json.dumps(report), 2, "tool_error"))
            for stdout, status, expected in cases:
                with self.subTest(expected=expected), mock.patch.object(module.subprocess, "run",
                    return_value=subprocess.CompletedProcess([], status, stdout, "diagnostic")):
                    output = root / expected
                    self.assertEqual(module.audit(["--file", str(lockfile)], output), 0)
                    metadata = json.loads((output / "metadata.json").read_text())
                    self.assertEqual(metadata["outcome"], expected)
                    self.assertEqual(metadata["raw_exit_code"], status)
                    self.assertEqual(metadata["policy"], "warning-only")
                    self.assertIsNotNone(metadata["lockfile_sha256"])
                    self.assertEqual((output / "Cargo.lock").read_bytes(), lockfile.read_bytes())

    def test_unsound_advisory_warns_only_for_affected_versions(self):
        script = pathlib.Path(__file__).with_name("audit_lockfile.py")
        with tempfile.TemporaryDirectory(prefix="salvo-audit-policy-") as directory:
            root = pathlib.Path(directory)
            database = root / "advisories"
            advisory = database / "crates" / "lru" / "RUSTSEC-2020-0001.md"
            advisory.parent.mkdir(parents=True)
            advisory.write_text(
                '```toml\n[advisory]\nid = "RUSTSEC-2020-0001"\n'
                'package = "lru"\ndate = "2020-01-01"\n'
                'informational = "unsound"\n[versions]\npatched = [">= 0.18.2"]\n'
                '```\n\n# Synthetic unsound advisory for the CI policy regression\n\n'
                'This fixture does not describe an additional real vulnerability.\n',
                encoding="utf-8",
            )
            for command in (
                ["git", "init", "--quiet"],
                ["git", "add", "."],
                [
                    "git", "-c", "user.name=Audit policy fixture",
                    "-c", "user.email=audit-fixture@example.invalid",
                    "commit", "--quiet", "-m", "Add synthetic advisory",
                ],
            ):
                subprocess.run(command, cwd=database, check=True)

            for version, affected in (("0.14.0", True), ("0.18.2", False)):
                with self.subTest(version=version):
                    lockfile = root / "Cargo.lock"
                    lockfile.write_text(
                        'version = 4\n\n[[package]]\nname = "lru"\n'
                        f'version = "{version}"\n'
                        'source = "registry+https://github.com/rust-lang/crates.io-index"\n',
                        encoding="utf-8",
                    )
                    result = subprocess.run(
                        [
                            sys.executable, str(script), "--file", str(lockfile),
                            "--db", str(database), "--no-fetch", "--stale", "--json",
                        ],
                        text=True, capture_output=True, check=False,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual("::warning" in result.stderr, affected)
                    report = json.loads(result.stdout)
                    unsound = report["warnings"].get("unsound", [])
                    self.assertEqual(len(unsound), 1 if affected else 0)
                    self.assertFalse(report["vulnerabilities"]["found"])


if __name__ == "__main__":
    unittest.main()
