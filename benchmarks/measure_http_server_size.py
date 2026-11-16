#!/usr/bin/env python3
"""Measure two real HTTP/1 servers; never reuse a previous measurement."""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import platform
import queue
import shutil
import socket
import subprocess
import threading
import time
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "benchmarks" / "http-server-size"


def capture(arguments, cwd=ROOT, timeout=60):
    return subprocess.run(arguments, cwd=cwd, check=True, capture_output=True,
                          text=True, timeout=timeout).stdout.strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def artifact_executable(log, package):
    executables = set()
    for line in log.read_text(encoding="utf-8").splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue  # stderr progress shares the build log.
        target = message.get("target", {})
        if (message.get("reason") == "compiler-artifact" and target.get("name") == package
                and "bin" in target.get("kind", []) and message.get("executable")):
            executables.add(Path(message["executable"]))
    if len(executables) != 1:
        raise RuntimeError(f"{log}: expected one {package} executable artifact")
    executable = executables.pop()
    if not executable.is_absolute() or not executable.is_file():
        raise RuntimeError(f"{log}: executable artifact is missing: {executable}")
    return executable


def build(arguments, cwd, log, timeout):
    started = time.monotonic()
    with log.open("w", encoding="utf-8") as output:
        try:
            result = subprocess.run(arguments, cwd=cwd, stdout=output,
                                    stderr=subprocess.STDOUT, timeout=timeout)
        except subprocess.TimeoutExpired as error:
            raise RuntimeError(f"build timed out; see {log}") from error
    if result.returncode:
        raise RuntimeError(f"build failed ({result.returncode}); see {log}")
    return round(time.monotonic() - started, 3)


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def ready_address(line):
    if not line.startswith("READY "):
        raise RuntimeError(f"unexpected server startup output: {line!r}")
    address = urlsplit(line[6:].strip())
    if (address.scheme != "http" or address.hostname != "127.0.0.1"
            or not address.port or address.path or address.query or address.fragment
            or address.username or address.password):
        raise RuntimeError(f"unexpected READY address: {line!r}")
    return address.hostname, address.port


def check_http(host, port, path, expected_head, timeout):
    statuses = {}
    for method, expected in (("GET", 200), ("POST", 405)):
        connection = http.client.HTTPConnection(host, port, timeout=timeout)
        try:
            connection.request(method, path, headers={"Connection": "close"})
            response = connection.getresponse()
            if response.status != expected:
                raise RuntimeError(f"{method} {path}: expected {expected}, got {response.status}")
            if method == "GET":
                body = response.read(12)  # One byte beyond the expected body detects excess.
                if body != b"Hello World":
                    raise RuntimeError(f"GET {path}: unexpected response body: {body!r}")
            statuses[method] = response.status
        finally:
            connection.close()
    # Read HEAD directly: HTTPResponse.read() suppresses an illegal HEAD body.
    with socket.create_connection((host, port), timeout=timeout) as connection:
        connection.sendall(f"HEAD {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".encode())
        response = bytearray()
        while chunk := connection.recv(4096):
            response.extend(chunk)
            if len(response) > 65536:
                raise RuntimeError("HEAD response exceeds smoke-check limit")
    headers, separator, body = bytes(response).partition(b"\r\n\r\n")
    status_line = headers.split(b"\r\n", 1)[0].split()
    if not separator or len(status_line) < 2 or status_line[1] != str(expected_head).encode():
        raise RuntimeError(f"HEAD {path}: expected {expected_head}, got {headers[:100]!r}")
    if body:
        raise RuntimeError(f"HEAD {path}: response contains body bytes")
    statuses["HEAD"] = expected_head
    return {"statuses": statuses, "get_body": "Hello World", "head_body_bytes": 0}


def smoke(arguments, path, expected_head, directory, timeout):
    with (directory / "server.stderr.log").open("w", encoding="utf-8") as stderr:
        process = subprocess.Popen(arguments, stdout=subprocess.PIPE, stderr=stderr,
                                   text=True, encoding="utf-8")
        lines = queue.Queue()
        expired = threading.Event()

        def expire():
            expired.set()
            try:
                process.kill()
            except OSError:
                pass  # The process may have exited at the deadline.

        # Bound the entire smoke check, including a response that keeps sending
        # bytes frequently enough to evade socket idle timeouts.
        deadline = threading.Timer(timeout, expire)
        deadline.start()

        def reader():
            with (directory / "server.stdout.log").open("w", encoding="utf-8") as output:
                for line in process.stdout:
                    output.write(line)
                    output.flush()
                    lines.put(line)
            lines.put(None)

        thread = threading.Thread(target=reader, daemon=True)
        thread.start()
        try:
            try:
                line = lines.get(timeout=timeout)
            except queue.Empty as error:
                raise RuntimeError("server did not announce READY before timeout") from error
            if line is None:
                raise RuntimeError(f"server exited before READY; see {directory}")
            host, port = ready_address(line)
            result = check_http(host, port, path, expected_head, timeout)
            if expired.is_set():
                raise RuntimeError("server smoke timed out")
            return result
        except Exception as error:
            if expired.is_set():
                raise RuntimeError(f"server smoke timed out; see {directory}") from error
            raise
        finally:
            deadline.cancel()
            stop(process)
            deadline.join(timeout=5)
            thread.join(timeout=5)
            process.stdout.close()


def provenance():
    source_files = [ROOT / "Cargo.toml", *sorted((ROOT / "crates").rglob("*.rs")),
                    *sorted((ROOT / "crates").rglob("Cargo.toml")),
                    *[FIXTURE / path for path in ("Cargo.toml", "salvo/Cargo.toml", "axum/Cargo.toml",
                                                  "salvo/src/main.rs", "axum/src/main.rs")],
                    Path(__file__).resolve()]
    hashes = {path.relative_to(ROOT).as_posix(): digest(path) for path in source_files}
    configuration = {}
    for ancestor in [FIXTURE, *FIXTURE.parents]:
        for name in ("config", "config.toml"):
            path = ancestor / ".cargo" / name
            if path.is_file():
                configuration[str(path)] = digest(path)
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    for name in ("config", "config.toml"):
        path = cargo_home / name
        if path.is_file():
            configuration[str(path)] = digest(path)
    environment = {key: value for key, value in os.environ.items()
                   if key in {"CARGO_BUILD_TARGET", "CARGO_TARGET_DIR", "CARGO_ENCODED_RUSTFLAGS",
                              "RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"}
                   or key.startswith("CARGO_PROFILE_")
                   or (key.startswith("CARGO_TARGET_") and key.endswith(("_LINKER", "_RUSTFLAGS", "_RUNNER")))}
    return {"revision": capture(["git", "rev-parse", "HEAD"]),
            "git_status": capture(["git", "status", "--porcelain", "--untracked-files=no"]),
            "rustc": capture(["rustc", "-Vv"]), "cargo": capture(["cargo", "-V"]),
            "platform": platform.platform(), "source_sha256": hashes,
            "lock_sha256": digest(FIXTURE / "Cargo.lock"),
            "cargo_config_sha256": configuration, "build_environment": environment}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument("--build-timeout", type=float, default=900)
    parser.add_argument("--smoke-timeout", type=float, default=10)
    args = parser.parse_args()
    if min(args.jobs, args.build_timeout, args.smoke_timeout) <= 0:
        parser.error("jobs and timeouts must be positive")
    metadata = json.loads(capture(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=FIXTURE))
    target = Path(metadata["target_directory"])
    output = target / "http-server-size" / f"{time.strftime('%Y%m%d-%H%M%S')}-{os.getpid()}"
    output.mkdir(parents=True, exist_ok=False)
    (output / "metadata.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    (output / "provenance.json").write_text(json.dumps(provenance(), indent=2), encoding="utf-8")
    rows = []
    print(f"Evidence: {output}", flush=True)
    for framework in ("salvo", "axum"):
        package = f"http-size-{framework}"
        for profile in ("release", "size"):
            for route in ("root", "hello"):
                name = f"{framework}-{profile}-{route}"
                directory = output / name
                directory.mkdir()
                command = ["cargo", "build", "--locked", "--profile", profile, "-p", package,
                           "-j", str(args.jobs), "--message-format=json"]
                if route == "hello":
                    command += ["--features", f"{package}/path-route"]
                log = directory / "build.log"
                print(f"Build {name}", flush=True)
                seconds = build(command, FIXTURE, log, args.build_timeout)
                cargo_executable = artifact_executable(log, package)
                executable = directory / cargo_executable.name
                shutil.copy2(cargo_executable, executable)
                row = {"framework": framework, "profile": profile, "route": route,
                       "command": command, "executable": str(executable),
                       "cargo_executable": str(cargo_executable),
                       "bytes": executable.stat().st_size, "sha256": digest(executable),
                       "build_seconds": seconds}
                row["smoke"] = smoke([str(executable)], "/" if route == "root" else "/hello",
                                     405 if framework == "salvo" else 200, directory, args.smoke_timeout)
                rows.append(row)
                (directory / "result.json").write_text(json.dumps(row, indent=2), encoding="utf-8")
                (output / "summary.json").write_text(json.dumps(rows, indent=2), encoding="utf-8")
                print(f"{name}: {row['bytes']} bytes; {row['smoke']['statuses']}", flush=True)
    print(f"Completed {len(rows)} measurements: {output / 'summary.json'}", flush=True)


if __name__ == "__main__":
    main()
