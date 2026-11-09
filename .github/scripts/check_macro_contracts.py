"""Compile a renamed downstream consumer and validate its generated OpenAPI document."""
import argparse
import copy
import json
from pathlib import Path
import subprocess

from jsonschema import Draft202012Validator


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=root, text=True))
    target = Path(metadata["target_directory"]).resolve()
    fixture = (target / "downstream-macro-contract").resolve()
    assert fixture.is_relative_to(target)
    (fixture / "src").mkdir(parents=True, exist_ok=True)
    manifest = '''[package]
name = "downstream-macro-contract"
version = "0.0.0"
edition = "2024"
publish = false
[workspace]
[dependencies]
renamed_framework = { package = "salvo_core", path = CORE_PATH, default-features = false, features = ["test"] }
renamed_api = { package = "salvo-oapi", path = OAPI_PATH }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
'''.replace("CORE_PATH", json.dumps(str(root / "crates/core"))).replace("OAPI_PATH", json.dumps(str(root / "crates/oapi")))
    (fixture / "Cargo.toml").write_text(manifest, encoding="utf-8")
    source = (root / ".github/fixtures/macro_contract.rs").read_text(encoding="utf-8")
    (fixture / "src/main.rs").write_text(source, encoding="utf-8")
    common = ["--manifest-path", str(fixture / "Cargo.toml")]
    if args.offline:
        common.append("--offline")
    result = subprocess.run(["cargo", "run", "--quiet", *common], cwd=root, text=True, capture_output=True)
    (fixture / "compile-pass.log").write_text(result.stderr, encoding="utf-8")
    if result.returncode:
        raise RuntimeError(result.stderr)
    document = json.loads(result.stdout)
    (fixture / "openapi.json").write_text(json.dumps(document, indent=2), encoding="utf-8")
    schema = json.loads((root / ".github/schemas/openapi-3.1.json").read_text(encoding="utf-8"))
    validator = Draft202012Validator(schema)
    validator.validate(document)
    # Independent standard constraints must reject known-invalid representations.
    for invalid in ("schema_and_content", "two_header_media_types"):
        bad = copy.deepcopy(document)
        operation = bad["paths"]["/contract"]["get"]
        if invalid == "schema_and_content":
            operation["parameters"][0]["schema"] = {"type":"string"}
        else:
            operation["responses"]["200"]["headers"]["x-contract"]["content"]["text/plain"] = {}
        if validator.is_valid(bad):
            raise AssertionError(f"standard schema unexpectedly accepted {invalid}")
    # This normative mutual exclusion is not encoded in the official schema.
    for parameter in document["paths"]["/contract"]["get"]["parameters"]:
        assert not ("example" in parameter and "examples" in parameter)
    cases = [
        ('use renamed_framework::prelude::*; #[derive(serde::Deserialize, Extractible)] struct Bad { #[serde(flatten)] value: std::collections::HashMap<String,String> } fn main() {}', '`#[serde(flatten)]` is not supported'),
        ('struct Generic<T>(T); #[renamed_api::endpoint] impl<T> Generic<T> { async fn run() {} } fn main() {}', 'does not support generic `impl` blocks'),
    ]
    try:
        for index, (code, expected) in enumerate(cases):
            (fixture / "src/main.rs").write_text(code, encoding="utf-8")
            result = subprocess.run(["cargo", "check", *common], cwd=root, text=True, capture_output=True)
            (fixture / f"compile-fail-{index}.log").write_text(result.stderr, encoding="utf-8")
            assert result.returncode and expected in result.stderr, result.stderr
    finally:
        (fixture / "src/main.rs").write_text(source, encoding="utf-8")
    print(f"Renamed consumer, standard document constraints and two compile-fail diagnostics passed; evidence: {fixture}")


if __name__ == "__main__":
    main()
