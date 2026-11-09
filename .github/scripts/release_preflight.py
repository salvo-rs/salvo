#!/usr/bin/env python3
"""Validate release versions and successful quality runs on exactly the tagged commit."""
import argparse
import json
import os
import re
import subprocess
import urllib.request

REQUIRED_WORKFLOWS = (
    ".github/workflows/linux.yml", ".github/workflows/windows.yml",
    ".github/workflows/macos.yml", ".github/workflows/format.yml",
)


def check_versions(metadata, tag):
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", tag):
        raise ValueError("release tag must be v followed by a complete package version")
    packages = {p["id"]: p for p in metadata["packages"]}
    for member in metadata["workspace_members"]:
        package = packages[member]
        if package["version"] != tag[1:]:
            raise ValueError(f"{package['name']} version {package['version']} does not match {tag}")


def check_runs(runs, sha):
    for path in REQUIRED_WORKFLOWS:
        candidates = [r for r in runs if r.get("path") == path and r.get("head_sha") == sha]
        if not candidates:
            raise ValueError(f"no quality run for {path} at tagged commit {sha}")
        latest = max(candidates, key=lambda r: (r.get("created_at", ""), r["id"]))
        if latest.get("status") != "completed" or latest.get("conclusion") != "success":
            raise ValueError(f"latest {path} run is not successful: {latest.get('html_url', latest['id'])}")


def read_runs(repo, sha):
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", repo):
        raise ValueError("invalid GitHub repository")
    token = os.environ.get("GITHUB_TOKEN")
    headers = {"Accept": "application/vnd.github+json", "User-Agent": "salvo-release-preflight"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    runs = []
    page = 1
    while True:
        request = urllib.request.Request(
            f"https://api.github.com/repos/{repo}/actions/runs?head_sha={sha}&per_page=100&page={page}",
            headers=headers,
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            batch = json.load(response)["workflow_runs"]
        runs.extend(batch)
        if len(batch) < 100:
            return runs
        page += 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--repo", required=True)
    args = parser.parse_args()
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    tagged = subprocess.check_output(["git", "rev-parse", f"refs/tags/{args.tag}^{{commit}}"], text=True).strip()
    if head != args.sha or tagged != args.sha:
        raise ValueError("checkout, tag, and event commit must match")
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], text=True))
    check_versions(metadata, args.tag)
    check_runs(read_runs(args.repo, args.sha), args.sha)
    print(f"Release preflight passed for {args.tag} at {args.sha}")


if __name__ == "__main__":
    main()
