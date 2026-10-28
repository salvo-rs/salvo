#!/usr/bin/env python3
"""Audit a lockfile, treating RustSec unsound advisories as failures."""

import subprocess
import sys


def audit(arguments):
    return subprocess.run(
        ["cargo", "audit", "--deny", "unsound", *arguments], check=False
    ).returncode


if __name__ == "__main__":
    raise SystemExit(audit(sys.argv[1:]))
