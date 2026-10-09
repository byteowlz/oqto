#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("platform_pin", Path(__file__).resolve().parents[1] / "release/sync-platform-pin.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PlatformPinTests(unittest.TestCase):
    def test_only_own_pin_changes_and_comments_survive(self):
        before = '# site notes\n[oqto]\nversion = "0.5.0" # keep\n\n[byteowlz]\nversion = "9.1.2"\n'
        expected = before.replace('"0.5.0"', '"0.5.6"')
        self.assertEqual(module.synchronize(before, "0.5.6"), expected)
        self.assertEqual(module.synchronize(expected, "0.5.6"), expected)

    def test_ambiguous_or_missing_pin_fails_closed(self):
        for text in ('[other]\nversion="1"\n', '[oqto]\nx=1\n', '[oqto]\nversion="1"\nversion="2"\n', '[oqto]\nversion="1"\n[oqto]\nversion="2"\n'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                module.synchronize(text, "0.5.6")

    def test_invalid_version_does_not_generate_content(self):
        with self.assertRaises(ValueError):
            module.synchronize('[oqto]\nversion="1"\n', 'latest')


if __name__ == '__main__':
    unittest.main()
