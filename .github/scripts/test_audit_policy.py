#!/usr/bin/env python3
"""Exercise the CI audit command against an offline, synthetic advisory DB."""

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest


class AuditPolicyTests(unittest.TestCase):
    def test_unsound_advisory_blocks_only_affected_versions(self):
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

            for version, expected in (("0.14.0", 1), ("0.18.2", 0)):
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
                    self.assertEqual(result.returncode, expected, result.stderr)
                    report = json.loads(result.stdout)
                    unsound = report["warnings"].get("unsound", [])
                    self.assertEqual(len(unsound), 1 if expected else 0)
                    self.assertFalse(report["vulnerabilities"]["found"])


if __name__ == "__main__":
    unittest.main()
