"""Check artifact selection, HTTP assertions, timeouts and server cleanup."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("server_sizes", Path(__file__).with_name("measure_http_server_size.py"))
sizes = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sizes)

SERVER = r'''
from http.server import HTTPServer, BaseHTTPRequestHandler
import sys
class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(sys.argv[1].encode())
    def do_HEAD(self):
        self.send_response(200)
        self.end_headers()
        if sys.argv[2] == 'body': self.wfile.write(b'illegal')
    def do_POST(self):
        self.send_response(405)
        self.end_headers()
server = HTTPServer(('127.0.0.1', 0), Handler)
print(f'READY http://127.0.0.1:{server.server_port}', flush=True)
server.serve_forever()
'''


class ServerSizing(unittest.TestCase):
    def test_artifact_uses_cargo_path_with_configured_target_and_spaces(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "custom target" / "fixture.exe"
            artifact.parent.mkdir()
            artifact.write_bytes(b"actual artifact")
            log = root / "build.log"
            log.write_text("Compiling fixture\n" + json.dumps({
                "reason": "compiler-artifact", "target": {"name": "fixture", "kind": ["bin"]},
                "executable": str(artifact)}), encoding="utf-8")
            self.assertEqual(sizes.artifact_executable(log, "fixture"), artifact)
            artifact.unlink()
            with self.assertRaisesRegex(RuntimeError, "artifact is missing"):
                sizes.artifact_executable(log, "fixture")

    def test_missing_or_ambiguous_artifact_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "build.log"
            for messages in ([], ["first", "second"]):
                log.write_text("\n".join(json.dumps({
                    "reason": "compiler-artifact", "target": {"name": "fixture", "kind": ["bin"]},
                    "executable": path}) for path in messages), encoding="utf-8")
                with self.assertRaisesRegex(RuntimeError, "expected one"):
                    sizes.artifact_executable(log, "fixture")

    def test_failed_build_retains_log(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "build.log"
            with self.assertRaisesRegex(RuntimeError, "build failed"):
                sizes.build([sys.executable, "-c", "print('diagnostic', flush=True); raise SystemExit(2)"],
                            directory, log, 5)
            self.assertIn("diagnostic", log.read_text())

    def test_build_timeout_reports_retained_log(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "build.log"
            with self.assertRaisesRegex(RuntimeError, "build timed out"):
                sizes.build([sys.executable, "-c", "import time; time.sleep(10)"], directory, log, .1)
            self.assertTrue(log.is_file())

    def test_http_success_wrong_get_and_illegal_head_body_all_stop_server(self):
        for body, head, expected_error in (("Hello World", "empty", None),
                                           ("wrong", "empty", "unexpected response body"),
                                           ("Hello World", "body", "body bytes")):
            with self.subTest(body=body, head=head), tempfile.TemporaryDirectory() as directory:
                processes = []
                original = sizes.subprocess.Popen
                def launch(*args, **kwargs):
                    process = original(*args, **kwargs)
                    processes.append(process)
                    return process
                with patch.object(sizes.subprocess, "Popen", side_effect=launch):
                    arguments = [sys.executable, "-u", "-c", SERVER, body, head]
                    if expected_error:
                        with self.assertRaisesRegex(RuntimeError, expected_error):
                            sizes.smoke(arguments, "/hello", 200, Path(directory), 5)
                    else:
                        result = sizes.smoke(arguments, "/hello", 200, Path(directory), 5)
                        self.assertEqual(result["statuses"], {"GET": 200, "POST": 405, "HEAD": 200})
                self.assertIsNotNone(processes[0].poll())

    def test_startup_timeout_and_early_exit_stop_server(self):
        for code, message in (("import time; time.sleep(10)", "timed out|announce READY"),
                              ("raise SystemExit(3)", "exited before READY")):
            with self.subTest(code=code), tempfile.TemporaryDirectory() as directory:
                with self.assertRaisesRegex(RuntimeError, message):
                    sizes.smoke([sys.executable, "-u", "-c", code], "/", 200, Path(directory), .3)

    def test_continuously_streaming_response_cannot_evade_smoke_deadline(self):
        streaming = SERVER.replace("self.wfile.write(sys.argv[1].encode())", """
        import time
        for byte in b'Hello World':
            self.wfile.write(bytes([byte]))
            self.wfile.flush()
            time.sleep(.1)
""")
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(RuntimeError, "smoke timed out"):
                sizes.smoke([sys.executable, "-u", "-c", streaming, "unused", "empty"],
                            "/", 200, Path(directory), .5)

    def test_ready_address_requires_local_dynamic_port(self):
        self.assertEqual(sizes.ready_address("READY http://127.0.0.1:54321\n"), ("127.0.0.1", 54321))
        for value in ("other output", "READY http://example.test:80", "READY http://127.0.0.1:0",
                      "READY http://127.0.0.1:80/path"):
            with self.assertRaises(RuntimeError):
                sizes.ready_address(value)


if __name__ == "__main__":
    unittest.main()
