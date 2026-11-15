"""The browser page gets the prepared 26.3 folder from the local server as one stream of frames
(web/prepared-resources.mjs), not one request per file: the folder holds 23 000 files. This pins
that wire format, which only the page and the server share: a frame is a 4-byte path length, the
UTF-8 path, a 4-byte size and the bytes, and the declared Content-Length covers all of it. It also
pins which prepared folder a server in this repository picks (locatePrepared): the one prepared
last, and only one that holds horizon-resources.json.
"""
import contextlib
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import tempfile
import time
import unittest
import urllib.error
import urllib.request

REPO = Path(__file__).resolve().parent.parent
SERVE = REPO / "web" / "serve.mjs"


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def wait_until_listening(port, process, timeout=20.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise AssertionError(f"web/serve.mjs exited: {process.stderr.read().decode(errors='replace')}")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise AssertionError(f"web/serve.mjs did not listen on {port}")


@contextlib.contextmanager
def serving(prepared):
    """web/serve.mjs on a free port, holding `prepared` instead of the repository's folder."""
    port = free_port()
    environment = dict(os.environ, PORT=str(port), HORIZON_WEB_RESOURCES=str(prepared))
    process = subprocess.Popen(
        [shutil.which("node"), str(SERVE)], cwd=REPO, env=environment,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    try:
        wait_until_listening(port, process)
        yield port
    finally:
        process.kill()
        process.communicate()


def frames(body):
    """{path: bytes} from the stream, as the page reads it."""
    files = {}
    at = 0
    while at < len(body):
        (path_length,) = struct.unpack_from("<I", body, at)
        at += 4
        path = body[at:at + path_length].decode("utf-8")
        at += path_length
        (size,) = struct.unpack_from("<I", body, at)
        at += 4
        files[path] = body[at:at + size]
        at += size
    return files


@unittest.skipUnless(shutil.which("node"), "node is needed to run the page's server")
class PreparedResourcesTests(unittest.TestCase):
    def test_stream_carries_every_file_under_its_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            prepared = Path(temporary) / "prepared"
            (prepared / "assets" / "minecraft" / "models").mkdir(parents=True)
            (prepared / "horizon-resources.json").write_text('{"version": "26.3"}')
            (prepared / "empty.bin").write_bytes(b"")
            payload = bytes(range(256)) * 3
            (prepared / "assets" / "minecraft" / "models" / "block.json").write_bytes(payload)
            with serving(prepared) as port:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/prepared-resources.bin") as response:
                    body = response.read()
                    headers = response.headers

            self.assertEqual(int(headers["X-Horizon-Files"]), 3)
            self.assertEqual(int(headers["Content-Length"]), len(body))
            self.assertEqual(frames(body), {
                "horizon-resources.json": b'{"version": "26.3"}',
                "empty.bin": b"",
                "assets/minecraft/models/block.json": payload,
            })

    def test_a_folder_that_is_not_there_is_a_404(self):
        with tempfile.TemporaryDirectory() as temporary:
            with serving(Path(temporary) / "missing") as port:
                with self.assertRaises(urllib.error.HTTPError) as caught:
                    urllib.request.urlopen(f"http://127.0.0.1:{port}/prepared-resources.bin")

        self.assertEqual(caught.exception.code, 404)
        caught.exception.close()


@unittest.skipUnless(shutil.which("node"), "node is needed to run the page's server")
class PreparedFolderDiscoveryTests(unittest.TestCase):
    """Which prepared folder a server inside this repository serves (locatePrepared)."""

    def locate(self, repository):
        module = (REPO / "web" / "prepared-resources.mjs").as_uri()
        script = (
            f"import({json.dumps(module)}).then(module => "
            f"console.log(module.locatePrepared(process.argv[1], null) ?? ''))"
        )
        result = subprocess.run(
            [shutil.which("node"), "-e", script, str(repository)],
            capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        found = result.stdout.strip()
        return Path(found) if found else None

    @staticmethod
    def same(path):
        return os.path.normcase(os.path.realpath(path)) if path else None

    def test_the_folder_prepared_last_is_served(self):
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            prepared = repository / ".local" / "horizon-client" / "26.3"
            copy = repository / "26.3"
            for folder, written in ((prepared, 1_000_000), (copy, 2_000_000)):
                folder.mkdir(parents=True)
                stamp = folder / "horizon-resources.json"
                stamp.write_text("{}")
                os.utime(stamp, (written, written))

            self.assertEqual(self.same(self.locate(repository)), self.same(copy))
            # A folder without horizon-resources.json is not a prepared folder, whatever its name.
            (copy / "horizon-resources.json").unlink()
            self.assertEqual(self.same(self.locate(repository)), self.same(prepared))

    def test_no_prepared_folder_means_the_page_keeps_its_picker(self):
        with tempfile.TemporaryDirectory() as temporary:
            self.assertIsNone(self.locate(Path(temporary)))


if __name__ == "__main__":
    unittest.main()
