import importlib.util
import json
import pathlib
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("summary", pathlib.Path(__file__).with_name("summarize_runtime_costs.py"))
summary = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(summary)


class ComparisonTests(unittest.TestCase):
    def test_compares_matched_cases_and_rejects_missing_or_duplicate_workloads(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            header = "case,p50_ns,p95_ns,p99_ns,allocations_per_operation\n"
            for label, row in (("before", "plain,100,110,120,0\n"),
                               ("after", "plain,120,130,150,1\n")):
                (root / label).mkdir()
                (root / label / "samples.csv").write_text(header + row)
            (root / "summary.json").write_text(json.dumps({"runtime": {
                "before": {"commit": "base"}, "after": {"commit": "head"}}}))
            report = summary.summarize(root)
            self.assertIn("+20.0%", report)
            self.assertIn("+25.0%", report)
            self.assertIn("0 → 1", report)
            path = root / "after" / "samples.csv"
            path.write_text(header + "different,1,1,1,1\n")
            with self.assertRaisesRegex(ValueError, "sets differ"):
                summary.summarize(root)
            path.write_text(header + "plain,1,1,1,1\nplain,1,1,1,1\n")
            with self.assertRaisesRegex(ValueError, "duplicate"):
                summary.summarize(root)
