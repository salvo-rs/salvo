"""Keep CI actions immutable and bound work without cancelling tag releases."""
import pathlib
import re
import unittest

import yaml


class UniqueKeyLoader(yaml.BaseLoader):
    def construct_mapping(self, node, deep=False):
        mapping = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in mapping:
                raise ValueError(f"duplicate YAML key {key!r} at line {key_node.start_mark.line + 1}")
            mapping[key] = self.construct_object(value_node, deep=deep)
        return mapping


class WorkflowPolicyTests(unittest.TestCase):
    def test_duplicate_workflow_keys_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "duplicate YAML key 'timeout-minutes'"):
            yaml.load("job:\n  timeout-minutes: 45\n  timeout-minutes: 45\n", Loader=UniqueKeyLoader)

    def test_workflows_pin_actions_and_bound_jobs(self):
        root = pathlib.Path(__file__).parents[1]
        for path in sorted((root / "workflows").glob("*.yml")):
            config = yaml.load(path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader)
            with self.subTest(workflow=path.name):
                concurrency = config["concurrency"]
                self.assertIn("github.workflow", concurrency["group"])
                # Tag-triggered publishing must not cancel an earlier release.
                self.assertIn(concurrency["cancel-in-progress"], ("false", "${{ github.ref_type != 'tag' }}"))
                for job in config["jobs"].values():
                    self.assertTrue(0 < int(job["timeout-minutes"]) <= 120)
                    for step in job.get("steps", []):
                        action = step.get("uses", "")
                        if action and not action.startswith("./"):
                            self.assertRegex(action, r"^[\w.-]+/[\w./-]+@[0-9a-f]{40}$")
        release = yaml.load((root / "workflows/release.yml").read_text(), Loader=UniqueKeyLoader)
        self.assertEqual(release["permissions"]["contents"], "read")
        self.assertEqual(release["jobs"]["release"]["permissions"]["contents"], "write")


if __name__ == "__main__":
    unittest.main()
