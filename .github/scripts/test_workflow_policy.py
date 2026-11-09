"""Keep CI actions immutable and bound work without cancelling tag releases."""
import pathlib
import re
import unittest

import yaml


class WorkflowPolicyTests(unittest.TestCase):
    def test_workflows_pin_actions_and_bound_jobs(self):
        root = pathlib.Path(__file__).parents[1]
        for path in sorted((root / "workflows").glob("*.yml")):
            config = yaml.load(path.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)
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
        release = yaml.load((root / "workflows/release.yml").read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(release["permissions"]["contents"], "read")
        self.assertEqual(release["jobs"]["release"]["permissions"]["contents"], "write")


if __name__ == "__main__":
    unittest.main()
