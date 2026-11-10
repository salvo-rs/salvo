#!/usr/bin/env python3
"""Run identical public-API workloads against two revisions and profile feature builds."""

import argparse
import csv
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time


ROOT = pathlib.Path(__file__).resolve().parents[1]


def command(arguments, cwd, output, env=None):
    started = time.perf_counter()
    with output.open("w", encoding="utf-8") as log:
        result = subprocess.run(arguments, cwd=cwd, env=env, stdout=log,
                                stderr=subprocess.STDOUT, check=False)
    if result.returncode:
        raise RuntimeError(f"{arguments[0]} failed ({result.returncode}); see {output}")
    return time.perf_counter() - started


def artifact_executable(log, target_name):
    executables = set()
    for line in log.read_text(encoding="utf-8").splitlines():
        try:
            artifact = json.loads(line)
        except json.JSONDecodeError:
            continue  # Cargo's progress on stderr shares this diagnostic log.
        target = artifact.get("target", {})
        if (artifact.get("reason") == "compiler-artifact"
                and target.get("name") == target_name
                and "bin" in target.get("kind", []) and artifact.get("executable")):
            executables.add(pathlib.Path(artifact["executable"]))
    if len(executables) != 1:
        raise RuntimeError(f"{log}: expected one executable artifact for {target_name}")
    executable = executables.pop()
    if not executable.is_absolute() or not executable.is_file():
        raise RuntimeError(f"{log}: Cargo executable artifact is missing: {executable}")
    return executable


def build_executable(arguments, cwd, log, target_name):
    seconds = command([*arguments, "--message-format=json"], cwd, log)
    return seconds, artifact_executable(log, target_name)


def capture(arguments, cwd=ROOT):
    return subprocess.run(arguments, cwd=cwd, check=True, capture_output=True,
                          text=True).stdout.strip()


def metadata(manifest):
    data = json.loads(capture(["cargo", "metadata", "--no-deps", "--format-version", "1",
                               "--manifest-path", str(manifest)]))
    project = manifest.parent.resolve()
    target = pathlib.Path(data["target_directory"]).resolve()
    # Every clean/build uses this fixture's normal target. Never clean a caller's shared target.
    if not target.is_relative_to(project):
        raise RuntimeError(f"fixture target {target} is outside {project}; keep the caller's setting and run without cleaning")
    return data


def revision_checkout(label, revision, output):
    sha = capture(["git", "rev-parse", f"{revision}^{{commit}}"])
    checkout = output / "revisions" / label
    checkout.parent.mkdir(parents=True, exist_ok=True)
    if checkout.exists():
        if capture(["git", "rev-parse", "HEAD"], checkout) != sha:
            raise RuntimeError(f"{checkout} already contains a different revision; choose another output directory")
    else:
        subprocess.run(["git", "worktree", "add", "--detach", str(checkout), sha], cwd=ROOT, check=True)
    destination = checkout / "benchmarks" / "runtime-costs"
    shutil.copytree(ROOT / "benchmarks" / "runtime-costs", destination,
                    ignore=shutil.ignore_patterns("target", "Cargo.lock"), dirs_exist_ok=True)
    return sha, checkout, destination / "Cargo.toml"


def validate_samples(path):
    with path.open(newline="", encoding="utf-8-sig") as source:
        rows = list(csv.reader(source))
    expected = "case,samples,p50_ns,p95_ns,p99_ns,operations_per_second,allocations_per_operation,allocated_bytes_per_operation".split(",")
    if not rows or rows[0] != expected or len(rows) != 16:
        raise RuntimeError(f"{path}: expected the full 15-case runtime matrix and its CSV schema")
    if len({row[0] for row in rows[1:]}) != 15:
        raise RuntimeError(f"{path}: duplicate runtime cases")
    for row in rows[1:]:
        if len(row) != len(expected) or int(row[1]) <= 0:
            raise RuntimeError(f"{path}: malformed sample row")
        if any(float(value) < 0 for value in row[2:]):
            raise RuntimeError(f"{path}: negative measurement")


def runtime_costs(args, output):
    records = {}
    first_lock = None
    first_fixture = None
    for label, revision in (("before", args.baseline), ("after", args.current)):
        sha, checkout, manifest = revision_checkout(label, revision, output)
        directory = output / label
        directory.mkdir(exist_ok=True)
        data = metadata(manifest)
        fixture = {str(path.relative_to(manifest.parent)): hashlib.sha256(path.read_bytes()).hexdigest()
                   for path in [manifest, *sorted((manifest.parent / "src").glob("*.rs"))]}
        if first_fixture is not None and fixture != first_fixture:
            raise RuntimeError("before/after fixture hashes differ; stop the comparison")
        first_fixture = fixture
        (directory / "fixture-sha256.json").write_text(json.dumps(fixture, indent=2), encoding="utf-8")
        (directory / "metadata.json").write_text(json.dumps(data, indent=2), encoding="utf-8")
        lock = manifest.parent / "Cargo.lock"
        if first_lock:
            shutil.copyfile(first_lock, lock)
        else:
            command(["cargo", "generate-lockfile", "--manifest-path", str(manifest)], checkout,
                    directory / "resolve.log")
        _, executable = build_executable(
            ["cargo", "build", "--release", "--locked", "-j", str(args.jobs),
             "--manifest-path", str(manifest)], checkout, directory / "build.log",
            "salvo-runtime-costs")
        shutil.copyfile(lock, directory / "Cargo.lock")
        first_lock = directory / "Cargo.lock"
        environment = os.environ.copy()
        environment["SALVO_BENCH_SAMPLES"] = str(args.samples)
        invocation = [str(executable)]
        if sys.platform.startswith("linux") and pathlib.Path("/usr/bin/time").exists():
            invocation = ["/usr/bin/time", "-v", "-o", str(directory / "resources.txt"), *invocation]
        seconds = command(invocation, checkout, directory / "samples.csv", environment)
        validate_samples(directory / "samples.csv")
        records[label] = {"commit": sha, "runtime_seconds": seconds,
                          "executable_bytes": executable.stat().st_size}
    return records


def feature_costs(args, output):
    manifest = ROOT / "benchmarks" / "feature-costs" / "Cargo.toml"
    data = metadata(manifest)
    target = pathlib.Path(data["target_directory"]).resolve()
    if target != (manifest.parent / "target").resolve():
        raise RuntimeError("feature profiling only cleans the fixture default target directory")
    records = {}
    for label, flags in (("minimal", []), ("full", ["--features", "full"])):
        directory = output / f"feature-{label}"
        directory.mkdir(exist_ok=True)
        command(["cargo", "generate-lockfile", "--manifest-path", str(manifest)], ROOT,
                directory / "resolve.log")
        # The metadata guard above verified this absolute target is fixture-local.
        command(["cargo", "clean", "--manifest-path", str(manifest)], ROOT, directory / "clean.log")
        build = ["cargo", "build", "--release", "--locked", "--timings", "-j", str(args.jobs),
                 "--manifest-path", str(manifest), *flags]
        cold, executable = build_executable(build, ROOT, directory / "cold.log", "salvo-feature-costs")
        timing = target / "cargo-timings"
        if timing.exists():
            shutil.copytree(timing, directory / "cargo-timings", dirs_exist_ok=True)
        warm, executable = build_executable(build, ROOT, directory / "incremental.log", "salvo-feature-costs")
        tree = capture(["cargo", "tree", "--locked", "--manifest-path", str(manifest),
                        "--edges", "normal,build", "--prefix", "none", *flags])
        (directory / "tree.txt").write_text(tree, encoding="utf-8")
        dependencies = {line.split(" (")[0] for line in tree.splitlines()}
        records[label] = {"cold_target_seconds": cold, "unchanged_incremental_seconds": warm,
                          "normal_build_package_versions_including_consumer": len(dependencies),
                          "smoke_consumer_executable_bytes": executable.stat().st_size}
        shutil.copyfile(manifest.parent / "Cargo.lock", directory / "Cargo.lock")
        (output / "feature-summary.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
    return records


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", default="e29dbf75e46311ab96acf92f96c16ca01434e15b")
    parser.add_argument("--current", default="HEAD")
    parser.add_argument("--samples", type=int, default=128)
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument("--skip-feature-costs", action="store_true")
    parser.add_argument("--output", type=pathlib.Path, default=ROOT / "target" / "perf-p06")
    args = parser.parse_args()
    if args.samples <= 0 or args.jobs <= 0:
        parser.error("samples and jobs must be positive")
    output = args.output.resolve()
    if not output.is_relative_to(ROOT):
        parser.error("keep output/revision worktrees inside this workspace")
    output.mkdir(parents=True, exist_ok=True)
    summary = {"rustc": capture(["rustc", "-Vv"]), "cargo": capture(["cargo", "-V"]),
               "samples": args.samples, "jobs": args.jobs}
    (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    summary["runtime"] = runtime_costs(args, output)
    (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    if not args.skip_feature_costs:
        summary["feature_builds"] = feature_costs(args, output)
    (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
