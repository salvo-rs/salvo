"""Safety checks for profiling's fixture-local clean and checkout reuse."""
import csv
import importlib.util
import json
import pathlib
import subprocess
import tempfile
import types
import tomllib
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("costs", pathlib.Path(__file__).with_name("measure_runtime_costs.py"))
costs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(costs)


class ProfilingSafety(unittest.TestCase):
    def test_lock_reuse_accepts_a_local_crate_version_bump(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            library = root / "library"
            library.mkdir()
            (library / "lib.rs").write_text("", encoding="utf-8")
            library_manifest = library / "Cargo.toml"
            library_manifest.write_text(
                '[package]\nname = "fixture-library"\nversion = "1.1.1"\n'
                '[lib]\npath = "lib.rs"\n', encoding="utf-8")
            manifest = root / "Cargo.toml"
            manifest.write_text(
                '[package]\nname = "fixture-consumer"\nversion = "0.0.0"\n'
                '[workspace]\n[lib]\npath = "lib.rs"\n'
                '[dependencies]\nfixture-library = { path = "library" }\n', encoding="utf-8")
            (root / "lib.rs").write_text("", encoding="utf-8")
            subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=root, check=True, capture_output=True)
            baseline = root / "baseline.lock"
            baseline.write_bytes((root / "Cargo.lock").read_bytes())
            library_manifest.write_text(library_manifest.read_text().replace('"1.1.1"', '"1.1.2"'), encoding="utf-8")
            costs.reuse_dependency_lock(baseline, manifest, root, root / "refresh.log")
            packages = tomllib.loads((root / "Cargo.lock").read_text())["package"]
            self.assertEqual(next(p["version"] for p in packages if p["name"] == "fixture-library"), "1.1.2")
            subprocess.run(["cargo", "metadata", "--offline", "--locked", "--format-version", "1"],
                           cwd=root, check=True, capture_output=True)

    def test_lock_reuse_rejects_external_dependency_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            baseline = root / "baseline.lock"
            baseline.write_text(
                'version = 4\n[[package]]\nname = "fixture"\nversion = "0.0.0"\n'
                '[[package]]\nname = "external"\nversion = "1.0.0"\n'
                'source = "registry+https://github.com/rust-lang/crates.io-index"\n', encoding="utf-8")
            def drift(*args):
                lock = root / "Cargo.lock"
                lock.write_text(lock.read_text().replace('"1.0.0"', '"2.0.0"'), encoding="utf-8")
            with patch.object(costs, "command", side_effect=drift):
                with self.assertRaisesRegex(RuntimeError, "external dependency locks differ"):
                    costs.reuse_dependency_lock(baseline, root / "Cargo.toml", root, root / "refresh.log")

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


class ExternalDependencyLocks(unittest.TestCase):
    SOURCE = "registry+https://github.com/rust-lang/crates.io-index"

    def package(self, name, version="1.0.0", **fields):
        return {"name": name, "version": version, "source": self.SOURCE,
                "checksum": "a" * 64, **fields}

    def reuse(self, before, after):
        def lock_text(packages):
            return "version = 4\n" + "".join(
                "[[package]]\n" + "".join(f"{key} = {json.dumps(value)}\n"
                                          for key, value in package.items())
                for package in packages)

        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            baseline = root / "baseline.lock"
            baseline.write_text(lock_text(before), encoding="utf-8")

            def prune(*args):
                (root / "Cargo.lock").write_text(lock_text(after), encoding="utf-8")

            with patch.object(costs, "command", side_effect=prune):
                return costs.reuse_dependency_lock(
                    baseline, root / "Cargo.toml", root, root / "refresh.log")

    def test_optional_regex_dependencies_can_be_pruned_and_are_reported(self):
        shared = self.package("bytes")
        removed = [self.package(name) for name in
                   ("regex-syntax", "regex", "aho-corasick", "regex-automata")]
        actual = self.reuse([shared, *removed], [shared])
        self.assertEqual(actual, [
            {field: package[field] for field in ("name", "version", "source")}
            for package in sorted(removed, key=lambda package: package["name"])])

    def test_identical_locks_have_no_removed_dependencies(self):
        packages = [self.package("bytes")]
        self.assertEqual(self.reuse(packages, packages), [])

    def test_pruning_does_not_allow_shared_package_drift(self):
        shared = self.package("bytes", dependencies=["shared-helper"])
        removed = self.package("regex")
        for field, value in (("version", "2.0.0"), ("source", "registry+https://other.invalid/index"),
                             ("checksum", "b" * 64), ("dependencies", [])):
            with self.subTest(field=field):
                with self.assertRaisesRegex(RuntimeError, "external dependency locks differ"):
                    self.reuse([shared, removed], [{**shared, field: value}])

    def test_new_external_dependencies_are_rejected(self):
        shared = self.package("bytes")
        with self.assertRaisesRegex(RuntimeError, "external dependency locks differ"):
            self.reuse([shared], [shared, self.package("unexpected")])


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
