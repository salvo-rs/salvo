#!/usr/bin/env python3
"""Exit 0 for a published version, 1 for absence, 2 when the index cannot be checked."""

import json
import http.client
import re
import sys
import urllib.error
import urllib.request


def index_path(crate):
    crate = crate.lower()
    if not re.fullmatch(r"[a-z0-9_-]+", crate):
        raise ValueError("invalid crate name")
    if len(crate) == 1:
        return f"1/{crate}"
    if len(crate) == 2:
        return f"2/{crate}"
    if len(crate) == 3:
        return f"3/{crate[0]}/{crate}"
    return f"{crate[:2]}/{crate[2:4]}/{crate}"


def check(crate, version):
    try:
        request = urllib.request.Request(
            f"https://index.crates.io/{index_path(crate)}",
            headers={"Accept": "text/plain", "User-Agent": "salvo-release-ci"},
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            # Read the whole successful response before searching. Never let a match
            # terminate the download, or interpret a partial download as absence.
            records = response.read().decode("utf-8").splitlines()
        versions = [json.loads(line)["vers"] for line in records if line.strip()]
        return 0 if version in versions else 1
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return 1
        print(f"Cannot check crates.io index: HTTP {error.code}", file=sys.stderr)
    except (OSError, http.client.HTTPException, ValueError, KeyError) as error:
        print(f"Cannot check crates.io index: {error}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: check_published.py CRATE VERSION")
    raise SystemExit(check(*sys.argv[1:]))
