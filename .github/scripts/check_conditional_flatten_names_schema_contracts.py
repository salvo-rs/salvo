"""Validate conditional flatten key names without relaxing map values or inactive fields."""
import json
import os
from pathlib import Path
import subprocess

from jsonschema import Draft202012Validator

root = Path(__file__).resolve().parents[2]
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=root, text=True
))
output = Path(metadata["target_directory"]) / "conditional-flatten-names-contracts.json"
subprocess.run(
    ["cargo", "test", "-p", "salvo-oapi-macros", "--test", "conditional_flatten_names_contracts",
     "--", "--include-ignored"],
    cwd=root,
    env={**os.environ, "SALVO_SCHEMA_CONTRACT_OUTPUT": str(output)},
    check=True,
)
cases = json.loads(output.read_text(encoding="utf-8"))
valid_count = 0
invalid_count = 0
for case in cases:
    Draft202012Validator.check_schema(case["schema"])
    validator = Draft202012Validator(case["schema"])
    for value in case["valid"]:
        errors = list(validator.iter_errors(value))
        assert not errors, (case["name"], value, [error.message for error in errors])
        valid_count += 1
    for value in case["invalid"]:
        assert not validator.is_valid(value), (case["name"], value)
        invalid_count += 1
print(f"Conditional flatten names contracts passed: {len(cases)} schemas, "
      f"{valid_count} valid, {invalid_count} invalid")
