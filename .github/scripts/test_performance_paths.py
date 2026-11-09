#!/usr/bin/env python3
"""Verify that edits to benchmarked implementations trigger both benchmark events."""

import pathlib
import unittest

import yaml


class PerformancePathsTests(unittest.TestCase):
    def test_implementation_and_benchmark_changes_trigger_both_events(self):
        workflow = pathlib.Path(__file__).parents[1] / "workflows" / "performance.yml"
        # BaseLoader keeps the GitHub Actions `on` key as a string (not a YAML 1.1 bool).
        config = yaml.load(workflow.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)
        for event in ("pull_request", "push"):
            patterns = config["on"][event]["paths"]
            for path in (
                "crates/core/src/routing/router.rs",
                "crates/core/src/service.rs",
                "crates/core/src/http/request.rs",
                "crates/macros/src/handler.rs",
                "crates/core/benches/router_detect.rs",
                "crates/core/benches/service_dispatch.rs",
                "Cargo.toml",
                "crates/core/Cargo.toml",
                "crates/macros/Cargo.toml",
                ".github/workflows/performance.yml",
                ".github/scripts/test_performance_paths.py",
            ):
                with self.subTest(event=event, path=path):
                    # The workflow uses only literal paths and recursive directory prefixes.
                    self.assertTrue(any(
                        path.startswith(pattern[:-2]) if pattern.endswith("/**")
                        else path == pattern for pattern in patterns
                    ), f"{event} does not run benchmarks for {path}")


if __name__ == "__main__":
    unittest.main()
