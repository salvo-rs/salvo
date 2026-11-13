"""Run macro consumers with facade/split dependencies and renamed dependency keys."""
import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--case", choices=["facade", "renamed-facade", "split", "renamed-split", "raw-identifier"])
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=root, text=True))
    target = Path(metadata["target_directory"]).resolve()
    fixture = (target / "macro-path-contract").resolve()
    assert fixture.is_relative_to(target)
    (fixture / "src").mkdir(parents=True, exist_ok=True)
    source = (root / ".github/fixtures/macro_paths.rs").read_text(encoding="utf-8")
    failures = []
    for case in ["facade", "renamed-facade", "split", "renamed-split", "raw-identifier"]:
        if args.case and args.case != case:
            continue
        facade = case in ("facade", "renamed-facade", "raw-identifier")
        framework = "framework" if case.startswith("renamed") else ("salvo" if facade else "salvo_core")
        api = "api" if case.startswith("renamed") else "salvo_oapi"
        package = "salvo" if facade else "salvo_core"
        path = root / "crates" / ("salvo" if facade else "core")
        manifest = '[package]\nname = "macro-path-contract"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n[workspace]\n[dependencies]\n'
        manifest += f'{framework} = {{ package = "{package}", path = {json.dumps(str(path))}, default-features = false, features = {json.dumps(["oapi", "test"] if facade else ["test"])} }}\n'
        if not facade:
            manifest += f'{api} = {{ package = "salvo-oapi", path = {json.dumps(str(root / "crates/oapi"))} }}\n'
        manifest += f'renamed_craft = {{ package = "salvo-craft", path = {json.dumps(str(root / "crates/craft"))} }}\n'
        manifest += 'serde = { version = "1", features = ["derive"] }\nserde_json = "1"\n'
        manifest += 'tokio = { version = "1", features = ["macros", "rt"] }\n'
        imports = f'use {framework} as framework;\n' if framework != "framework" else ""
        imports += f'use {framework}::oapi as api;\n' if facade else (f'use {api} as api;\n' if api != "api" else "")
        code = source
        if case == "raw-identifier":
            code = '''#[api::endpoint]
async fn r#match(r#type: api::extract::QueryParam<String>) -> String { r#type.into_inner() }
fn main() {
    let router = framework::Router::with_path("raw").get(r#match);
    let document = serde_json::to_value(api::OpenApi::new("raw", "1").merge_router(&router)).unwrap();
    assert_eq!(document["paths"]["/raw"]["get"]["parameters"][0]["name"], "type");
}
'''
        (fixture / "Cargo.toml").write_text(manifest, encoding="utf-8")
        (fixture / "src/main.rs").write_text(imports + code, encoding="utf-8")
        command = ["cargo", "run", "--quiet", "--manifest-path", str(fixture / "Cargo.toml")]
        if args.offline:
            command.append("--offline")
        result = subprocess.run(command, cwd=root, text=True, capture_output=True)
        (fixture / f"{case}.log").write_text(result.stderr, encoding="utf-8")
        print(f'{case}: {"FAIL" if result.returncode else "PASS"}', flush=True)
        if result.returncode:
            failures.append(case)
    if failures:
        raise SystemExit(f"Failed: {', '.join(failures)}; evidence: {fixture}")
    print(f"Macro dependency path contracts passed; evidence: {fixture}")


if __name__ == "__main__":
    main()
