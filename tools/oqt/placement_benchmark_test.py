#!/usr/bin/env python3
"""Hermetic regression tests: no installed runtime, engine or user profiles used."""
import json
import sys
import unittest
from unittest.mock import patch
import placement_benchmark as bench


class BenchmarkTest(unittest.TestCase):
    def test_nearest_rank_and_unavailable(self):
        self.assertIsNone(bench.percentile([], .95))
        self.assertEqual(bench.percentile([5, 1, 2, 4, 3], .5), 3)
        self.assertEqual(bench.percentile([5, 1, 2, 4, 3], .95), 5)
        self.assertEqual(bench.stats([{'status': 'timeout', 'duration_ms': 500}]),
                         {'n': 0, 'failures': 1, 'p50_ms': None, 'p95_ms': None})

    @patch.object(bench.shutil, 'which', return_value=None)
    def test_absent_schema(self, _):
        result = bench.benchmark(collect_inventory=False)
        self.assertEqual(result['schema_version'], 1)
        self.assertEqual(set(result['placements']), set(bench.PLACEMENTS))
        for metrics in result['placements'].values():
            self.assertEqual(set(metrics), set(bench.METRICS))
            for metric, value in metrics.items():
                self.assertEqual(value['samples'], [])
                self.assertIsNone(value['stats']['p95_ms'])
                self.assertEqual(value['status'], 'unsupported' if metric == 'queued_input_acceptance' else 'not_measured')
        json.dumps(result, allow_nan=False)

    def test_run_timeout_and_no_secret_export(self):
        record, _ = bench.run([sys.executable, '-c', 'import time; print("secret"); time.sleep(3)'], .05)
        self.assertEqual(record['status'], 'timeout')
        self.assertGreater(record['end_ns'], record['start_ns'])
        self.assertNotIn('secret', json.dumps(record))

    def test_run_limits(self):
        record, data = bench.run([sys.executable, '-c',
            'import os,json; print(json.dumps([len(os.sched_getaffinity(0)),os.getpriority(os.PRIO_PROCESS,0)]))'])
        self.assertEqual(record['status'], 'ok')
        self.assertEqual(json.loads(data), [1, 19])

    @patch.object(bench.shutil, 'which', return_value='/fixture/oqt')
    @patch.object(bench, 'run')
    def test_verification_not_readiness(self, run, _):
        run.return_value = ({'status': 'ok', 'duration_ms': 7, 'start_ns': 1, 'end_ns': 7000001},
                            json.dumps(dict(artifact='fixture', runtime='/fixture', source='/fixture/src', node='/fixture/node')).encode())
        # Return a fresh record per invocation, like the real executor.
        run.side_effect = lambda *a: (dict(run.return_value[0]), run.return_value[1])
        result = bench.benchmark(3, collect_inventory=False)
        local = result['placements']['local_process']
        self.assertEqual(local['verification']['stats']['n'], 3)
        self.assertEqual(local['verification']['by_phase']['warm']['n'], 2)
        self.assertEqual(local['guard_ready']['status'], 'not_measured')
        self.assertTrue(all(call.args[0] == ['/fixture/oqt', '--runtime-info'] for call in run.call_args_list))

    def test_receipt_identity_not_visible_order(self):
        identity = '12345678-1234-1234-1234-123456789abc:operator:abcdef01-1234-1234-1234-123456789abc'
        other = '87654321-1234-1234-1234-123456789abc:operator:abcdef02-1234-1234-1234-123456789abc'
        command = 'true # unique-fixture'
        raw = (f'{other}: admitted\n$ true # unrelated\n'
               f'\x1b[32m{identity}: admitted\x1b[0m\n$ {command}\n').encode()
        self.assertEqual(bench.receipt_identity(raw, command), identity)
        with self.assertRaisesRegex(RuntimeError, 'identity_binding'):
            bench.receipt_identity(raw, 'true # absent')
        with self.assertRaisesRegex(RuntimeError, 'identity_binding'):
            bench.receipt_identity(raw + f'{other}: admitted\n$ {command}\n'.encode(), command)

    def test_summary_separates_cold_warm_and_origins(self):
        report = {'placements': {'local_standalone_oqt': {'guard_ready': {
            'status': 'measured', 'reason': 'protected RPC upper bound',
            'stats': {'n': 2}, 'origin': 'PTY launch', 'interpretation': 'includes probe overhead',
            'by_phase': {'cold': {'n': 1, 'p50_ms': 10, 'p95_ms': 10},
                         'warm': {'n': 1, 'p50_ms': 5, 'p95_ms': 5}}}}}}
        text = bench.summary(report)
        self.assertIn('guard_ready (cold)', text)
        self.assertIn('guard_ready (warm)', text)
        self.assertIn('includes probe overhead', text)
        self.assertNotIn('n=2', text)

    @patch.object(bench.os, 'pidfd_open', return_value=123)
    @patch.object(bench.signal, 'pidfd_send_signal')
    def test_cleanup_refuses_foreign_owner(self, send, opened):
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'p/fixture').mkdir(parents=True)
            (root/'p/fixture/host.json').write_text(json.dumps({'pid': 1}))
            with patch.object(Path, 'read_bytes', return_value=b'foreign'), patch.object(bench.os, 'close'):
                with self.assertRaisesRegex(RuntimeError, 'unproven_fixture_host'):
                    bench.retire_fixture(Path('/fixture/runtime'), Path('/fixture/owner'), root)
            send.assert_not_called()

    def test_missing_executable(self):
        record, data = bench.run(['/no/such/fixture'])
        self.assertEqual(record['status'], 'unavailable')
        self.assertIsNone(record['duration_ms'])
        self.assertEqual(data, b'')


if __name__ == '__main__':
    unittest.main()
