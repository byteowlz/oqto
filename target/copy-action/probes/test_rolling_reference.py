"""Narrow harness regression tests; full probe asserts reconstruction independently."""

import unittest

from rolling_reference import parse_stats


class StatsTests(unittest.TestCase):
    def test_rsync_344_mixed_unit_suffixes(self):
        stats = parse_stats(
            "Literal data: 4,096 bytes\nMatched data: 134,217,728 bytes\n"
            "Total bytes sent: 12,345\nTotal bytes received: 67,890\n"
        )
        self.assertEqual(
            stats,
            {
                "literal_bytes": 4096,
                "matched_bytes": 134217728,
                "sent_bytes": 12345,
                "received_bytes": 67890,
            },
        )

    def test_missing_stat_refuses_partial_measurement(self):
        with self.assertRaises(ValueError):
            parse_stats("Literal data: 4,096 bytes\n")


if __name__ == "__main__":
    unittest.main()
