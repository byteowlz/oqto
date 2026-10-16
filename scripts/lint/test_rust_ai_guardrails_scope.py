"""Regression checks for Rust's real outer/inner cfg(test) boundaries."""

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "guardrails_report", Path(__file__).with_name("rust-ai-guardrails-report.py")
)
REPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPORT)


class TestScope(unittest.TestCase):
    def assert_scope(self, source, expected):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "fixture.rs").write_text(source)
            with mock.patch.object(REPORT, "ROOT", root):
                REPORT._first_cfg_test_line.cache_clear()
                for line, in_test in expected:
                    self.assertEqual(
                        REPORT._is_test_context("fixture.rs", line), in_test
                    )
                REPORT._first_cfg_test_line.cache_clear()

    def test_inner_attribute_marks_the_whole_module_test_only(self):
        self.assert_scope("#![cfg(test)]\nfn helper() {}\n", [(0, True), (1, True)])

    def test_outer_test_block_does_not_hide_prior_production(self):
        self.assert_scope(
            "fn production() {}\n#[cfg(test)]\nmod tests {}\n",
            [(0, False), (1, True), (2, True)],
        )

    def test_other_feature_attributes_do_not_exclude_production(self):
        self.assert_scope(
            '#![cfg(feature = "test")]\nfn production() {}\n', [(0, False), (1, False)]
        )

    def test_missing_source_does_not_exclude_production(self):
        with (
            tempfile.TemporaryDirectory() as temp,
            mock.patch.object(REPORT, "ROOT", Path(temp)),
        ):
            REPORT._first_cfg_test_line.cache_clear()
            self.assertFalse(REPORT._is_test_context("missing.rs", 1))
            REPORT._first_cfg_test_line.cache_clear()

    def test_unexpected_read_bug_is_not_swallowed(self):
        REPORT._first_cfg_test_line.cache_clear()
        with (
            mock.patch.object(
                Path, "read_text", side_effect=RuntimeError("synthetic bug")
            ),
            self.assertRaises(RuntimeError),
        ):
            REPORT._first_cfg_test_line("fixture.rs")
        REPORT._first_cfg_test_line.cache_clear()


if __name__ == "__main__":
    unittest.main()
