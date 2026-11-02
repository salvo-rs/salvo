#!/usr/bin/env python3
"""Audit a lockfile and report findings as non-blocking CI warnings."""

import subprocess
import sys


def audit(arguments):
    status = subprocess.run(
        ["cargo", "audit", "--deny", "unsound", *arguments], check=False
    ).returncode
    if status:
        print(
            "::warning title=Dependency audit::cargo audit reported dependency "
            "advisories or could not complete. Review the audit output; "
            "this warning does not block Salvo releases.",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(audit(sys.argv[1:]))
