"""Validate derived untagged enum schemas against their actual serde payloads."""
import json
from pathlib import Path
import subprocess
import sys

from jsonschema import Draft202012Validator


def main():
    root = Path(__file__).resolve().parents[2]
    run = subprocess.run(
        ["cargo", "test", "-p", "salvo-oapi-macros", "--test", "untagged_enum_semantics", "--", "--nocapture"],
        cwd=root, capture_output=True, text=True,
    )
    if run.returncode:
        print(run.stdout, file=sys.stderr)
        print(run.stderr, file=sys.stderr)
        raise SystemExit(run.returncode)
    prefix = "UNTAGGED_ENUM_CONTRACT="
    position = run.stdout.find(prefix)
    if position < 0:
        raise SystemExit("The enum contract fixture was not emitted")
    cases = json.loads(run.stdout[position + len(prefix):].splitlines()[0])
    invalid_values = {
        "numeric": ["invalid", False],
        "objects": [{}, {"value": 2}],
        "nulls": ["invalid"],
        "external": [None, []],
        "adjacent": [None, []],
        "internal": [None, []],
        "unit": [1, {"kind": "Other"}],
        "external_unit": [1, "Other"],
    }
    accepted = 0
    rejected = 0
    for name, case in cases.items():
        Draft202012Validator.check_schema(case["schema"])
        validator = Draft202012Validator(case["schema"])
        for payload in case["values"]:
            validator.validate(payload)
            accepted += 1
        for payload in invalid_values[name]:
            if validator.is_valid(payload):
                raise AssertionError(f"{name} unexpectedly accepts {payload!r}")
            rejected += 1
    print(f"Enum schema contracts passed: {accepted} valid payloads and {rejected} invalid payloads across {len(cases)} representations")


if __name__ == "__main__":
    main()
