"""Validate the benchmark CSV schema before publishing its evidence."""
import csv
import pathlib
import sys

FIELDS = "case,samples,p50_ns,p95_ns,p99_ns,operations_per_second,allocations_per_operation,allocated_bytes_per_operation,encoded_bytes,input_bytes,compression_ratio".split(",")


def validate(path):
    with pathlib.Path(path).open(newline="", encoding="utf-8-sig") as source:
        rows = list(csv.reader(source))
    assert rows and rows[0] == FIELDS, f"{path}: unexpected CSV header"
    assert len(rows) == 73, f"{path}: expected all 72 workload rows"
    cases = set()
    for index, row in enumerate(rows[1:], 2):
        assert len(row) == len(FIELDS), f"{path}:{index}: expected {len(FIELDS)} columns, got {len(row)}"
        assert row[0] not in cases, f"{path}:{index}: duplicate case"
        cases.add(row[0])
        assert int(row[1]) > 0 and int(row[9]) == 65536
        assert int(row[8]) > 0
        assert abs(float(row[10]) - int(row[8]) / int(row[9])) <= 0.000051
    return len(cases)


if __name__ == "__main__":
    for argument in sys.argv[1:]:
        print(f"{argument}: {validate(argument)} cases validated")
