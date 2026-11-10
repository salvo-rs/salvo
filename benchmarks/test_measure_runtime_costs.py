"""Safety checks for profiling's fixture-local clean and checkout reuse."""
import csv
import importlib.util
import json
import pathlib
import tempfile
import types
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("costs", pathlib.Path(__file__).with_name("measure_runtime_costs.py"))
costs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(costs)


class ProfilingSafety(unittest.TestCase):
    def test_external_target_is_rejected_without_clean(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = pathlib.Path(directory) / "fixture" / "Cargo.toml"
            with patch.object(costs, "capture", return_value='{"target_directory":"/shared-target"}'):
                with self.assertRaisesRegex(RuntimeError, "outside"):
                    costs.metadata(manifest)

    def test_nondefault_fixture_target_is_never_cleaned(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            target = root / "benchmarks" / "feature-costs" / "custom-target"
            with patch.object(costs, "ROOT", root), patch.object(costs, "metadata", return_value={"target_directory": str(target)}), patch.object(costs, "command") as command:
                with self.assertRaisesRegex(RuntimeError, "default target"):
                    costs.feature_costs(types.SimpleNamespace(jobs=2), root)
                command.assert_not_called()

    def test_existing_different_revision_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory)
            (output / "revisions" / "before").mkdir(parents=True)
            with patch.object(costs, "capture", side_effect=["expected", "existing"]), patch.object(costs.shutil, "copytree") as copy:
                with self.assertRaisesRegex(RuntimeError, "different revision"):
                    costs.revision_checkout("before", "revision", output)
                copy.assert_not_called()

    def test_failed_command_keeps_diagnostic_log(self):
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory) / "failure.log"
            with patch.object(costs.subprocess, "run", return_value=types.SimpleNamespace(returncode=2)):
                with self.assertRaisesRegex(RuntimeError, r"failed \(2\)"):
                    costs.command(["compiler"], directory, output)
            self.assertTrue(output.exists())


class ExecutableArtifacts(unittest.TestCase):
    def check_configured_target(self, name):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            executable = root / "target" / "configured-host-triple" / "release" / name
            executable.parent.mkdir(parents=True)
            executable.write_bytes(b"configured target executable")
            # A stale default-target executable must never be selected.
            stale = root / "target" / "release" / name
            stale.parent.mkdir(parents=True)
            stale.write_bytes(b"stale")
            log = root / "build.jsonl"
            messages = [
                {"reason": "compiler-artifact", "target": {"name": "dependency", "kind": ["bin"]}, "executable": str(stale)},
                {"reason": "compiler-artifact", "target": {"name": name, "kind": ["bin"]}, "executable": None},
                {"reason": "compiler-artifact", "target": {"name": name, "kind": ["bin"]}, "executable": str(executable)},
                {"reason": "build-finished", "success": True},
            ]
            log.write_text("Cargo progress on stderr\n" + "\n".join(map(json.dumps, messages)), encoding="utf-8")
            with patch.object(costs, "command", return_value=1.0) as command:
                seconds, actual = costs.build_executable(["cargo", "build", "--release"], root, log, name)
            self.assertEqual(actual, executable)
            self.assertEqual(seconds, 1.0)
            self.assertIn("--message-format=json", command.call_args.args[0])
            self.assertNotIn("--target", command.call_args.args[0])
            self.assertEqual(actual.stat().st_size, len(b"configured target executable"))

    def test_runtime_uses_configured_target_artifact(self):
        self.check_configured_target("salvo-runtime-costs")

    def test_feature_size_uses_configured_target_artifact(self):
        self.check_configured_target("salvo-feature-costs")

    def test_missing_executable_artifact_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            log = pathlib.Path(directory) / "build.jsonl"
            log.write_text('{"reason":"build-finished","success":true}')
            with self.assertRaisesRegex(RuntimeError, "expected one executable"):
                costs.artifact_executable(log, "salvo-runtime-costs")


class SampleEvidence(unittest.TestCase):
    def test_missing_row_or_column_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory) / "samples.csv"
            header = "case,samples,p50_ns,p95_ns,p99_ns,operations_per_second,allocations_per_operation,allocated_bytes_per_operation".split(",")
            rows = [header, *[[str(index), "128", "1", "2", "3", "4", "0", "0"] for index in range(15)]]
            for malformed in [rows[:-1], [header, rows[1][:-1], *rows[2:]]]:
                with output.open("w", newline="", encoding="utf-8") as source:
                    csv.writer(source).writerows(malformed)
                with self.assertRaises(RuntimeError):
                    costs.validate_samples(output)
            with output.open("w", newline="", encoding="utf-8") as source:
                csv.writer(source).writerows(rows)
            costs.validate_samples(output)


if __name__ == "__main__":
    unittest.main()
