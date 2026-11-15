"""The site installs as an app: web/manifest.webmanifest names the icons the browser shows, and
web/sw.js is the worker that keeps the page, Emscripten's loader and the wasm on the player's disk.
This pins what the browser is handed: a manifest whose declared icon paths exist with an image
type, and a worker served fresh (`no-store`), so a rebuilt game replaces the cached one.
"""
import json
import shutil
import tempfile
import unittest
import urllib.request
from pathlib import Path

try:  # `unittest discover -s tests` puts tests/ itself on the path; `tests.xxx` runs do not.
    from test_web_prepared_resources import serving
except ModuleNotFoundError:
    from tests.test_web_prepared_resources import serving


@unittest.skipUnless(shutil.which("node"), "node is needed to run the page's server")
class InstallableAppTests(unittest.TestCase):
    def get(self, port, path):
        with urllib.request.urlopen(f"http://127.0.0.1:{port}{path}") as response:
            return response.headers, response.read()

    def test_the_manifest_and_the_icons_it_names_are_served(self):
        with tempfile.TemporaryDirectory() as temporary:
            with serving(Path(temporary)) as port:
                headers, body = self.get(port, "/manifest.webmanifest")
                manifest = json.loads(body)

                self.assertEqual(headers["Content-Type"], "application/manifest+json; charset=utf-8")
                self.assertEqual(manifest["start_url"], "/")
                self.assertEqual(manifest["display"], "standalone")
                self.assertTrue(manifest["icons"], "the install offer needs at least one icon")
                for icon in manifest["icons"]:
                    icon_headers, icon_body = self.get(port, icon["src"])
                    self.assertEqual(icon_headers["Content-Type"], icon["type"], icon["src"])
                    self.assertTrue(icon_body, icon["src"])

    def test_the_service_worker_is_served_fresh(self):
        with tempfile.TemporaryDirectory() as temporary:
            with serving(Path(temporary)) as port:
                headers, body = self.get(port, "/sw.js")

        self.assertEqual(headers["Content-Type"], "text/javascript; charset=utf-8")
        # A cached worker would keep answering with the old cache name after a rebuild.
        self.assertEqual(headers["Cache-Control"], "no-store")
        self.assertTrue(body, "the worker must reach the browser")


if __name__ == "__main__":
    unittest.main()
