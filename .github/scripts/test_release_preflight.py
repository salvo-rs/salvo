import re
import tomllib
import unittest
from pathlib import Path

from release_preflight import REQUIRED_WORKFLOWS, check_runs, check_versions


class ReleasePreflightTests(unittest.TestCase):
    def test_publish_order_includes_versioned_development_dependencies(self):
        root = Path(__file__).resolve().parents[2]
        workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
        packages = {}
        for member in workspace["members"]:
            for directory in root.glob(member):
                manifest = tomllib.loads((directory / "Cargo.toml").read_text(encoding="utf-8"))
                if manifest["package"].get("publish") is not False:
                    packages[manifest["package"]["name"]] = (directory, manifest)

        workflow = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
        publish_array = re.search(r"crates=\((.*?)\n\s*\)", workflow, re.DOTALL)
        self.assertIsNotNone(publish_array, "release workflow must declare its publish order")
        entries = re.findall(r'"([^":]+):([^"\n]+)"', publish_array.group(1))
        positions = {name: index for index, (name, _) in enumerate(entries)}
        self.assertEqual(len(positions), len(entries), "publish order has duplicate crates")
        self.assertEqual(set(positions), set(packages), "publish every workspace crate exactly once")
        for name, relative in entries:
            directory, manifest = packages[name]
            self.assertEqual(root / relative, directory)
            scopes = [manifest, *manifest.get("target", {}).values()]
            for scope in scopes:
                for kind in ("dependencies", "build-dependencies", "dev-dependencies"):
                    for alias, dependency in scope.get(kind, {}).items():
                        if isinstance(dependency, str):
                            dependency = {"version": dependency}
                        if dependency.get("workspace"):
                            inherited = workspace["dependencies"][alias]
                            if isinstance(inherited, str):
                                inherited = {"version": inherited}
                            dependency = {**inherited, **dependency}
                        target = dependency.get("package", alias)
                        if target not in positions:
                            continue
                        # Unversioned path dev-dependencies are omitted from packages;
                        # wildcard dev-dependencies can resolve an earlier release.
                        if kind == "dev-dependencies" and dependency.get("version", "*") == "*":
                            continue
                        self.assertLess(
                            positions[target], positions[name],
                            f"{name}: {kind} dependency {target} must be published first",
                        )

    def test_versions_match_every_workspace_member(self):
        metadata = {"workspace_members":["a", "b"], "packages":[
            {"id":"a", "name":"one", "version":"1.2.3"}, {"id":"b", "name":"two", "version":"1.2.3"}]}
        check_versions(metadata, "v1.2.3")
        for tag in ["v1.2", "v1.2.4", "not-a-release"]:
            with self.assertRaises(ValueError): check_versions(metadata, tag)
        metadata["packages"][1]["version"] = "1.2.4"
        with self.assertRaises(ValueError): check_versions(metadata, "v1.2.3")

    def test_only_successful_latest_runs_at_exact_commit_pass(self):
        runs = [{"path":path, "event":"push", "id":i, "head_sha":"commit", "created_at":"1", "status":"completed", "conclusion":"success"}
                for i,path in enumerate(REQUIRED_WORKFLOWS)]
        check_runs(runs, "commit")
        with self.assertRaises(ValueError): check_runs([{**r, "event":"pull_request"} for r in runs], "commit")
        # A newer synthetic PR merge run is not evidence for the tagged tree.
        check_runs(runs + [{**runs[0], "event":"pull_request", "id":200, "created_at":"3", "conclusion":"failure"}], "commit")
        with self.assertRaises(ValueError): check_runs(runs, "other-commit")
        with self.assertRaises(ValueError): check_runs(runs[:-1], "commit")
        for state, conclusion in [("in_progress",None), ("completed","failure"), ("completed","cancelled")]:
            later = {**runs[0], "id":100, "created_at":"2", "status":state, "conclusion":conclusion}
            with self.assertRaises(ValueError): check_runs(runs+[later], "commit")


if __name__ == "__main__":
    unittest.main()
