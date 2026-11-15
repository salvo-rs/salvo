"""Distinguish recursive declarations with identical package/version/type names."""
import json
from pathlib import Path
import subprocess

from jsonschema import Draft202012Validator

root = Path(__file__).resolve().parents[2]
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=root, text=True))
target = Path(metadata["target_directory"]).resolve()
fixture = (target / "recursive-declaration-contract").resolve()
assert fixture.is_relative_to(target)
(fixture / "src").mkdir(parents=True, exist_ok=True)
api = json.dumps(str(root / "crates/oapi"))
version = next(package["version"] for package in metadata["packages"] if package["name"] == "salvo-oapi")
sources = []
for label, field in [("one", "legacy: String"), ("two", "modern: u32")]:
    model = fixture / ("model-" + label)
    (model / "src").mkdir(parents=True, exist_ok=True)
    (model / "Cargo.toml").write_text(f'''[package]
name = "shared-model"
version = "0.0.1"
edition = "2024"
[workspace]
[dependencies]
salvo-oapi = {{ version = "={version}", default-features = false }}
''', encoding="utf-8")
    (model / "src/lib.rs").write_text('''#![allow(dead_code)]
#[derive(salvo_oapi::ToSchema)]
pub struct Node<T> {
    value: T,
    children: Vec<Node<T>>,
    FIELD,
}
'''.replace("FIELD", field), encoding="utf-8")
    subprocess.run(["git", "init", "--quiet", str(model)], check=True)
    subprocess.run(["git", "-C", str(model), "add", "Cargo.toml", "src/lib.rs"], check=True)
    changed = subprocess.run(["git", "-C", str(model), "diff", "--cached", "--quiet"])
    if changed.returncode == 1:
        subprocess.run(["git", "-C", str(model), "-c", "commit.gpgsign=false", "-c", "user.name=Schema fixture",
                        "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "fixture"], check=True)
    elif changed.returncode:
        raise RuntimeError("cannot inspect fixture Git source")
    sources.append(model.as_uri())

(fixture / "Cargo.toml").write_text(f'''[package]
name = "recursive-declaration-contract"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
api = {{ package = "salvo-oapi", path = {api}, default-features = false }}
one = {{ package = "shared-model", git = {json.dumps(sources[0])} }}
two = {{ package = "shared-model", git = {json.dumps(sources[1])} }}
serde_json = "1"
[patch.crates-io]
salvo-oapi = {{ path = {api} }}
salvo-oapi-macros = {{ path = {json.dumps(str(root / "crates/oapi-macros"))} }}
salvo_core = {{ path = {json.dumps(str(root / "crates/core"))} }}
salvo_macros = {{ path = {json.dumps(str(root / "crates/macros"))} }}
salvo-serde-util = {{ path = {json.dumps(str(root / "crates/serde-util"))} }}
''', encoding="utf-8")
(fixture / "src/main.rs").write_text('''use api::{Components, ComposeSchema, Object};
use serde_json::json;
fn main() {
    assert_eq!(std::any::type_name::<one::Node<String>>(), std::any::type_name::<two::Node<String>>());
    assert_ne!(one::Node::<String>::__compose_declaration_id(), two::Node::<String>::__compose_declaration_id());
    let mut components = Components::new();
    let first = one::Node::<String>::compose(&mut components, vec![]);
    let second = two::Node::<String>::compose(&mut components, vec![]);
    assert!(json!(first)["properties"].get("legacy").is_some());
    assert!(json!(second)["properties"].get("modern").is_some());
    assert_eq!(components.schemas.len(), 2);
    let schema = Object::new().property("first", first).property("second", second)
        .required("first").required("second");
    println!("{}", json!({"schema":{"allOf":[schema],"components":components},
        "valid":[{"first":{"value":"one","legacy":"old","children":[]},
                  "second":{"value":"two","modern":2,"children":[{"value":"child","modern":3,"children":[]}]}}],
        "invalid":[{"first":{"value":"one","legacy":"old","children":[]},
                    "second":{"value":"two","modern":2,"children":[{"value":"child","legacy":"wrong","children":[]}]}}]}));
}
''', encoding="utf-8")
# Refresh only this generated consumer's lockfile after updating its local Git sources.
(fixture / "Cargo.lock").unlink(missing_ok=True)
subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1",
                "--manifest-path", str(fixture / "Cargo.toml")], cwd=root, check=True, stdout=subprocess.DEVNULL)
result = subprocess.run(["cargo", "run", "--quiet", "--manifest-path", str(fixture / "Cargo.toml")],
                        cwd=root, text=True, capture_output=True)
(fixture / "compile.log").write_text(result.stderr, encoding="utf-8")
if result.returncode:
    raise RuntimeError(result.stderr)
case = json.loads(result.stdout)
(fixture / "schema.json").write_text(json.dumps(case, indent=2), encoding="utf-8")
Draft202012Validator.check_schema(case["schema"])
validator = Draft202012Validator(case["schema"])
for value in case["valid"]:
    validator.validate(value)
for value in case["invalid"]:
    assert not validator.is_valid(value), value
print("Same-name/version recursive declarations from distinct Git sources stay independent")
