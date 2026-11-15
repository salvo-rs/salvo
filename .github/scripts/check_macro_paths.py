"""Run macro consumers with facade/split dependencies and renamed dependency keys."""
import argparse
import json
from pathlib import Path
import subprocess

CASES = [
    "facade", "renamed-facade", "split", "renamed-split", "raw-identifier",
    "shadow-facade", "shadow-renamed-facade", "shadow-split", "shadow-renamed-split", "mixed-shadow",
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--case", choices=CASES)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=root, text=True))
    target = Path(metadata["target_directory"]).resolve()
    fixture = (target / "macro-path-contract").resolve()
    assert fixture.is_relative_to(target)
    (fixture / "src").mkdir(parents=True, exist_ok=True)
    source = (root / ".github/fixtures/macro_paths.rs").read_text(encoding="utf-8")
    failures = []
    for case in CASES:
        if args.case and args.case != case:
            continue
        base_case = case.removeprefix("shadow-")
        mixed = case == "mixed-shadow"
        shadowed = case.startswith("shadow-") or mixed
        facade = base_case in ("facade", "renamed-facade", "raw-identifier") or mixed
        renamed = base_case.startswith("renamed") or mixed
        framework = "framework" if renamed else ("salvo" if facade else "salvo_core")
        api = "api" if renamed else "salvo_oapi"
        package = "salvo" if facade else "salvo_core"
        path = root / "crates" / ("salvo" if facade else "core")
        manifest = '[package]\nname = "macro-path-contract"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n[workspace]\n[dependencies]\n'
        manifest += f'{framework} = {{ package = "{package}", path = {json.dumps(str(path))}, default-features = false, features = {json.dumps(["oapi", "test"] if facade and not mixed else ["test"])} }}\n'
        if not facade or mixed:
            manifest += f'{api} = {{ package = "salvo-oapi", path = {json.dumps(str(root / "crates/oapi"))} }}\n'
        manifest += f'renamed_craft = {{ package = "salvo-craft", path = {json.dumps(str(root / "crates/craft"))} }}\n'
        manifest += 'serde = { version = "1", features = ["derive"] }\nserde_json = "1"\n'
        manifest += 'tokio = { version = "1", features = ["macros", "rt"] }\n'
        imports = f'use {framework} as framework;\n' if framework != "framework" else ""
        imports += f'use {framework}::oapi as api;\n' if facade and not mixed else (f'use {api} as api;\n' if api != "api" else "")
        code = source
        if shadowed:
            api_path = f"::{framework}::oapi::" if facade and not mixed else f"::{api}::"
            code = code.replace("framework::", f"::{framework}::").replace("api::", api_path)
            code = code.replace("renamed_craft::", "::renamed_craft::")
            code = code.replace("async fn main()", "pub async fn run()")
            code = (
                f"#[allow(dead_code)] mod shadowed {{ mod {framework} {{}} mod {api} {{}} mod renamed_craft {{}} mod std {{}}\n"
                + code + "\n}\nfn main() { shadowed::run(); }\n"
            )
            imports = ""
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
