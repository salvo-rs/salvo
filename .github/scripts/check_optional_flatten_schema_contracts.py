"""Validate absent/present flattened groups while rejecting partial and mistyped children."""
import json
import os
from pathlib import Path
import subprocess
from jsonschema import Draft202012Validator

root = Path(__file__).resolve().parents[2]
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=root, text=True))
output = Path(metadata["target_directory"]) / "optional-flatten-contracts.json"
subprocess.run(["cargo", "test", "-p", "salvo-oapi-macros", "--test", "optional_flatten_contracts"], cwd=root, env={**os.environ, "SALVO_SCHEMA_CONTRACT_OUTPUT": str(output)}, check=True)
for case in json.loads(output.read_text(encoding="utf-8")):
    Draft202012Validator.check_schema(case["schema"])
    validator = Draft202012Validator(case["schema"])
    for value in case["valid"]:
        validator.validate(value)
    for value in case["invalid"]:
        assert not validator.is_valid(value), value
print("Optional flatten serde/schema contracts passed")
