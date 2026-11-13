"""Summarize matched workload measurements; no noisy shared-runner release gate."""
import csv
import json
import math
import os
import pathlib
import sys


def read_cases(path):
    with path.open(newline="", encoding="utf-8-sig") as source:
        rows = list(csv.DictReader(source))
    result = {}
    for row in rows:
        if row["case"] in result:
            raise ValueError(f"duplicate workload: {row['case']}")
        for field in ("p50_ns", "p95_ns", "p99_ns", "allocations_per_operation"):
            value = float(row[field])
            if not math.isfinite(value) or value < 0:
                raise ValueError(f"invalid {field} for {row['case']}")
        result[row["case"]] = row
    if not result:
        raise ValueError("empty workload output")
    return result


def summarize(directory):
    before = read_cases(directory / "before" / "samples.csv")
    after = read_cases(directory / "after" / "samples.csv")
    if before.keys() != after.keys():
        raise ValueError("before/after workload sets differ")
    evidence = json.loads((directory / "summary.json").read_text(encoding="utf-8"))
    lines = ["# Runtime comparison", "",
             f"Base: `{evidence['runtime']['before']['commit']}`",
             f"Current: `{evidence['runtime']['after']['commit']}`", "",
             "Same runner and fixture; values include client/assertion overhead. "
             "Positive latency changes mean slower. Timing differences are informational, "
             "not statistical significance or a release gate.", "",
             "| Workload | Base p50 ns | Current p50 ns | p50 change | p99 change | Allocations base → current |",
             "| --- | ---: | ---: | ---: | ---: | ---: |"]
    def change(old, new):
        return f"{(float(new) / float(old) - 1) * 100:+.1f}%" if float(old) else "n/a"
    for name, row in before.items():
        current = after[name]
        lines.append(f"| {name} | {row['p50_ns']} | {current['p50_ns']} | "
                     f"{change(row['p50_ns'], current['p50_ns'])} | "
                     f"{change(row['p99_ns'], current['p99_ns'])} | "
                     f"{row['allocations_per_operation']} → {current['allocations_per_operation']} |")
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    directory = pathlib.Path(sys.argv[1])
    report = summarize(directory)
    (directory / "comparison.md").write_text(report, encoding="utf-8")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as destination:
            destination.write(report)
    print(report)
