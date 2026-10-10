#!/usr/bin/env python3
"""`just bump` must change only tauri.conf.json's version, never its formatting."""

import importlib.util
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "set_tauri_version", ROOT / "scripts/release/set-tauri-version.py"
)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SetTauriVersion(unittest.TestCase):
    def test_checked_in_file_changes_only_the_version_line(self) -> None:
        original = (ROOT / "frontend/src-tauri/tauri.conf.json").read_text()
        updated = module.set_version(original, "9.8.7")
        self.assertEqual(json.loads(updated)["version"], "9.8.7")
        changed = [
            (a, b)
            for a, b in zip(original.splitlines(), updated.splitlines())
            if a != b
        ]
        self.assertEqual(len(changed), 1, changed)
        self.assertEqual(len(original.splitlines()), len(updated.splitlines()))

    def test_nested_versions_are_not_mistaken_for_the_top_level(self) -> None:
        text = '{\n\t"version": "1.0.0",\n\t"plugins": {\n\t\t"x": { "version": "3.0.0" }\n\t}\n}\n'
        updated = module.set_version(text, "1.0.1")
        self.assertEqual(json.loads(updated)["version"], "1.0.1")
        self.assertEqual(json.loads(updated)["plugins"]["x"]["version"], "3.0.0")

    def test_missing_top_level_version_fails(self) -> None:
        with self.assertRaises(ValueError):
            module.set_version('{\n\t"plugins": { "version": "3.0.0" }\n}\n', "1.0.1")


if __name__ == "__main__":
    unittest.main()
