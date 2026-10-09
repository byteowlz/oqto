#!/usr/bin/env python3
"""Lock cross-origin isolation into every generated Caddy frontend profile."""
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
HEADERS = {
    "Cross-Origin-Opener-Policy": "same-origin",
    "Cross-Origin-Embedder-Policy": "require-corp",
}


def frontend_profiles():
    setup = (ROOT / "scripts/setup/11-production.sh").read_text()
    profiles = re.findall(r"<<EOF\n(# Oqto Caddyfile.*?)(?:\nEOF)", setup, re.S)
    docker = (ROOT / "deploy/docker/entrypoint.sh").read_text()
    match = re.search(r"cat > /etc/caddy/Caddyfile <<EOF\n(.*?)\nEOF", docker, re.S)
    if len(profiles) != 2 or match is None:
        raise AssertionError("Expected local, production and Docker Caddy profiles")
    return [*profiles, match[1]]


class SpeechIsolation(unittest.TestCase):
    def test_all_generated_frontends_are_isolated(self):
        for index, profile in enumerate(frontend_profiles()):
            with self.subTest(profile=index):
                header_blocks = re.findall(r"(?m)^\s*header \{([^}]+)\}", profile)
                for name, value in HEADERS.items():
                    matches = re.findall(rf"(?m)^\s*{name}\s+(\S+)\s*$", profile)
                    self.assertEqual(matches, [value], name)
                    self.assertTrue(
                        any(f"{name} {value}" in block for block in header_blocks),
                        "Headers must apply to the whole frontend, including workers",
                    )
                self.assertIn("file_server", profile)
                self.assertIn("reverse_proxy", profile)


if __name__ == "__main__":
    unittest.main()
