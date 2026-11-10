"""Exercise both real fixtures using a caller-configured native Cargo build target."""
import os
import pathlib

import measure_runtime_costs as costs


def main():
    host = next(line.split(": ", 1)[1] for line in costs.capture(["rustc", "-Vv"]).splitlines()
                if line.startswith("host: "))
    if os.environ.get("CARGO_BUILD_TARGET") != host:
        raise RuntimeError("configure CARGO_BUILD_TARGET to rustc's host triple for this executable smoke test")
    output = costs.ROOT / "target" / "perf-p06" / "configured-target"
    output.mkdir(parents=True, exist_ok=True)
    for fixture in ("runtime", "feature"):
        name = f"salvo-{fixture}-costs"
        manifest = costs.ROOT / "benchmarks" / f"{fixture}-costs" / "Cargo.toml"
        metadata = costs.metadata(manifest)
        if not manifest.with_name("Cargo.lock").exists():
            costs.command(["cargo", "generate-lockfile", "--manifest-path", str(manifest)], costs.ROOT,
                          output / f"{fixture}-resolve.log")
        _, executable = costs.build_executable(
            ["cargo", "build", "--release", "--locked", "-j", "2", "--manifest-path", str(manifest)],
            costs.ROOT, output / f"{fixture}-build.jsonl", name)
        if not executable.is_relative_to(pathlib.Path(metadata["target_directory"])) or host not in executable.parts:
            raise RuntimeError(f"expected a native configured-target artifact, got {executable}")
        costs.command([str(executable)], costs.ROOT, output / f"{fixture}-execution.log")
        if fixture == "runtime":
            costs.validate_samples(output / f"{fixture}-execution.log")
        print(f"{fixture}: {executable} ({executable.stat().st_size} bytes)", flush=True)


if __name__ == "__main__":
    main()
