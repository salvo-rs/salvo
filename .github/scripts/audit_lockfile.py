#!/usr/bin/env python3
"""Audit a lockfile, preserving warning-only policy and optional reproducible evidence."""

import argparse
import datetime
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys


def audit(arguments, evidence_dir=None):
    command = ["cargo", "audit", "--deny", "unsound", *arguments]
    if evidence_dir is not None and "--json" not in command:
        command.append("--json")
    try:
        result = subprocess.run(command, check=False, text=True, capture_output=True)
        stdout, stderr, status = result.stdout, result.stderr, result.returncode
    except OSError as error:
        stdout, stderr, status = "", str(error) + "\n", None
    sys.stdout.write(stdout)
    sys.stderr.write(stderr)

    outcome = None
    if evidence_dir is not None:
        try:
            outcome = save_evidence(pathlib.Path(evidence_dir), command, arguments, stdout, stderr, status)
        except OSError as error:
            outcome = "tool_error"
            print(f"::warning title=Audit evidence::Could not save audit evidence: {error}", file=sys.stderr)
    if status != 0 or outcome == "tool_error":
        print(
            "::warning title=Dependency audit::cargo audit reported dependency "
            "advisories or could not complete. Review the audit output; "
            "this warning does not block Salvo releases.",
            file=sys.stderr,
        )
    return 0


def save_evidence(directory, command, arguments, stdout, stderr, status):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "audit.json").write_text(stdout, encoding="utf-8")
    (directory / "audit.stderr.log").write_text(stderr, encoding="utf-8")
    try:
        report = json.loads(stdout)
    except (ValueError, TypeError):
        report = {}
    if not isinstance(report, dict):
        report = {}
    complete = status in (0, 1) and all(key in report for key in ("database", "lockfile", "vulnerabilities", "warnings"))
    vulnerabilities = report.get("vulnerabilities", {})
    warnings = report.get("warnings", {})
    has_findings = bool(vulnerabilities.get("found") or any(warnings.values()))
    outcome = ("findings" if has_findings else "clean") if complete else "tool_error"
    if status != 0 and not has_findings:
        outcome = "tool_error"
    if "--file" in arguments and arguments.index("--file") + 1 < len(arguments):
        lockfile = pathlib.Path(arguments[arguments.index("--file") + 1])
    else:
        lockfile = pathlib.Path(next((arg.split("=", 1)[1] for arg in arguments if arg.startswith("--file=")), "Cargo.lock"))
    lock_hash = None
    if lockfile.is_file():
        lock_hash = hashlib.sha256(lockfile.read_bytes()).hexdigest()
        shutil.copyfile(lockfile, directory / "Cargo.lock")
    metadata = {
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "outcome": outcome,
        "raw_exit_code": status,
        "policy": "warning-only",
        "command": command,
        "lockfile": str(lockfile),
        "lockfile_sha256": lock_hash,
        "database": report.get("database"),
        "ignored_advisories": report.get("settings", {}).get("ignore", []),
        "dependency_count": report.get("lockfile", {}).get("dependency-count"),
        "advisories": [entry["advisory"]["id"] for entry in vulnerabilities.get("list", [])],
        "warnings": {kind: [entry["advisory"]["id"] for entry in entries] for kind, entries in warnings.items()},
    }
    (directory / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return outcome


if __name__ == "__main__":
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--evidence-dir")
    options, forwarded = parser.parse_known_args()
    raise SystemExit(audit(forwarded, options.evidence_dir))
