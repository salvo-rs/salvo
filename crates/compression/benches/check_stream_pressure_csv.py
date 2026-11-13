"""Validate the benchmark CSV schema before publishing its evidence."""
import csv
import argparse
import itertools
import pathlib

FIELDS = "case,samples,p50_ns,p95_ns,p99_ns,operations_per_second,allocations_per_operation,allocated_bytes_per_operation,encoded_bytes,input_bytes,compression_ratio".split(",")


def validate(path, granularity=False):
    with pathlib.Path(path).open(newline="", encoding="utf-8-sig") as source:
        rows = list(csv.reader(source))
    assert rows and rows[0] == FIELDS, f"{path}: unexpected CSV header"
    chunks = [64, 1024, 16384, 1048576] if granularity else [8192, 16384, 65536]
    consumers = ["ready"] if granularity else ["ready", "slow"]
    concurrency = [1] if granularity else [1, 8]
    expected = {
        f"{algorithm}/{level}/{chunk}/{consumer}/{parallel}"
        for algorithm, level, chunk, consumer, parallel in itertools.product(
            ["gzip", "br", "zstd"], ["default", "precise5"], chunks, consumers, concurrency
        )
    }
    assert len(rows) == len(expected) + 1, f"{path}: expected all {len(expected)} workload rows"
    cases = set()
    for index, row in enumerate(rows[1:], 2):
        assert len(row) == len(FIELDS), f"{path}:{index}: expected {len(FIELDS)} columns, got {len(row)}"
        assert row[0] not in cases, f"{path}:{index}: duplicate case"
        cases.add(row[0])
        assert int(row[1]) > 0 and int(row[9]) == (1048576 if granularity else 65536)
        assert int(row[8]) > 0
        assert abs(float(row[10]) - int(row[8]) / int(row[9])) <= 0.000051
    assert cases == expected, f"{path}: unexpected workload matrix"
    return len(cases)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--granularity", action="store_true")
    parser.add_argument("paths", nargs="+")
    arguments = parser.parse_args()
    for path in arguments.paths:
        print(f"{path}: {validate(path, arguments.granularity)} cases validated")
