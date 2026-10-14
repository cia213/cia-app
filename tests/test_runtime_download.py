import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest


HELPER = Path(__file__).resolve().parents[1] / "src-tauri/resources/bootstrap/bootstrap-python.ps1"
PAYLOAD = bytes(range(256)) * 512


class ArtifactServer(BaseHTTPRequestHandler):
    requests = []

    def log_message(self, *args):
        pass

    def do_GET(self):
        type(self).requests.append((self.path, self.headers.get("Range")))
        payload = b"<html>not a model</html>" if self.path == "/invalid" else PAYLOAD
        offset = 0
        if self.path != "/ignore-range" and self.headers.get("Range"):
            offset = int(self.headers["Range"].removeprefix("bytes=").removesuffix("-"))
        self.send_response(206 if offset else 200)
        if offset:
            self.send_header("Content-Range", f"bytes {offset}-{len(payload)-1}/{len(payload)}")
        self.send_header("Content-Length", str(len(payload) - offset))
        self.end_headers()
        self.wfile.write(payload[offset:])


@unittest.skipUnless(os.name == "nt", "Windows PowerShell bootstrap transport")
class RuntimeDownloadTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), ArtifactServer)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def download(self, root, endpoint):
        destination = root / "artifact.bin"
        script = root / "download.ps1"
        escaped_helper = str(HELPER).replace("'", "''")
        script.write_text("$ErrorActionPreference='Stop'\n. '" + escaped_helper + "'\n"
                          "Get-RifeArtifact -Url 'http://127.0.0.1:" + str(self.server.server_port) + endpoint + "' "
                          "-Destination (Join-Path $PSScriptRoot 'artifact.bin') -Hash '" + hashlib.sha256(PAYLOAD).hexdigest() + "' -Label 'test artifact'\n")
        return subprocess.run(["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(script)],
                              capture_output=True, timeout=25, text=True), destination

    def test_resumes_interrupted_verified_download(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "artifact.bin.part").write_bytes(PAYLOAD[:4096])
            result, destination = self.download(root, "/payload")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(destination.read_bytes(), PAYLOAD)
            self.assertIn(("/payload", "bytes=4096-"), ArtifactServer.requests)
            self.assertFalse((root / "artifact.bin.part").exists())

    def test_server_ignoring_resume_restarts_safely(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "artifact.bin.part").write_bytes(b"incorrect prefix")
            result, destination = self.download(root, "/ignore-range")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(destination.read_bytes(), PAYLOAD)

    def test_complete_partial_is_promoted_without_request(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            before = len(ArtifactServer.requests)
            (root / "artifact.bin.part").write_bytes(PAYLOAD)
            result, destination = self.download(root, "/payload")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(destination.read_bytes(), PAYLOAD)
            self.assertEqual(len(ArtifactServer.requests), before)

    def test_corrupt_cached_destination_is_replaced(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "artifact.bin").write_bytes(b"corrupt existing artifact")
            result, destination = self.download(root, "/payload")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(destination.read_bytes(), PAYLOAD)

    def test_hash_failure_never_promotes_download(self):
        with tempfile.TemporaryDirectory() as directory:
            before = len(ArtifactServer.requests)
            root = Path(directory)
            result, destination = self.download(root, "/invalid")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("SHA-256", result.stderr)
            self.assertEqual(len(ArtifactServer.requests) - before, 3)
            self.assertFalse(destination.exists())
            self.assertFalse((root / "artifact.bin.part").exists())


if __name__ == "__main__":
    unittest.main()
