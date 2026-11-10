"""Exercise the real coverage wrapper with a failing cargo subprocess."""
import os
import pathlib
import subprocess
import tempfile
import unittest


class CoverageFailureTests(unittest.TestCase):
    def test_failed_coverage_keeps_exit_status_environment_lock_and_log(self):
        script = pathlib.Path(__file__).with_name("run_coverage.sh").resolve()
        shell = "C:/Program Files/Git/bin/bash.exe" if os.name == "nt" else "bash"
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            commands = root / "commands"
            commands.mkdir()
            for name in ("git", "rustc"):
                stub = commands / name
                stub.write_text("#!/bin/sh\necho fixture-tool\n", newline="\n")
                stub.chmod(0o755)
            cargo = commands / "cargo"
            cargo.write_text('#!/bin/sh\ncase "$1" in\n generate-lockfile) echo fixture > Cargo.lock;;\n tarpaulin) if [ "$2" = --version ]; then echo "cargo-tarpaulin test"; else echo "fixture coverage failure"; exit 7; fi;;\n *) echo "cargo test";;\nesac\n', newline="\n")
            cargo.chmod(0o755)
            invocation = [shell, str(script)]
            if os.name == "nt":
                invocation = [shell, "-c", 'export PATH="$(cygpath -u "$1"):$PATH"; exec bash "$2"', "coverage-test", str(commands), str(script)]
            result = subprocess.run(invocation, cwd=root,
                env={**os.environ, "PATH": str(commands) + os.pathsep + os.environ["PATH"]},
                capture_output=True, text=True)
            self.assertEqual(result.returncode, 7, result.stderr + result.stdout + (root / "coverage-artifacts/environment.txt").read_text())
            output = root / "coverage-artifacts"
            self.assertIn("cargo-tarpaulin test", (output / "environment.txt").read_text())
            self.assertEqual((output / "Cargo.lock").read_text(), "fixture\n")
            self.assertIn("fixture coverage failure", (output / "tarpaulin.log").read_text())


if __name__ == "__main__":
    unittest.main()
