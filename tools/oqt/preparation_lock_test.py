#!/usr/bin/env python3
"""Real-process lock proof; no builds, live profiles or installed runtime."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

from preparation_lock import acquire_preparation_lock


class PreparationLockTests(unittest.TestCase):
    def test_second_builder_waits_and_exec_does_not_inherit_lock(self):
        with tempfile.TemporaryDirectory(prefix='oqt-lock-proof-') as temporary:
            root = Path(temporary)
            cache = root / 'cache'
            descriptor = acquire_preparation_lock(cache)
            child = subprocess.Popen([
                sys.executable, '-c',
                'import os,sys; from pathlib import Path; '
                'from preparation_lock import acquire_preparation_lock; '
                'root=Path(sys.argv[1]); (root/"started").write_text("ready"); '
                'fd=acquire_preparation_lock(root/"cache"); '
                '(root/"acquired").write_text("locked"); os.close(fd)',
                str(root),
            ], env={**os.environ, 'PYTHONPATH': str(Path(__file__).resolve().parent)}, close_fds=True)
            try:
                deadline = time.monotonic() + 5
                while not (root / 'started').exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue((root / 'started').exists())
                time.sleep(0.1)
                self.assertIsNone(child.poll())
                self.assertFalse((root / 'acquired').exists())
                os.close(descriptor)
                descriptor = None
                self.assertEqual(child.wait(timeout=5), 0)
                self.assertEqual((root / 'acquired').read_text(), 'locked')
            finally:
                if descriptor is not None:
                    os.close(descriptor)
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=5)

    def test_public_or_symlinked_entries_are_refused_without_overwriting(self):
        with tempfile.TemporaryDirectory(prefix='oqt-lock-invalid-') as temporary:
            root = Path(temporary)
            public = root / 'public'
            public.mkdir(mode=0o755)
            public.chmod(0o755)
            with self.assertRaises(RuntimeError):
                acquire_preparation_lock(public)
            private = root / 'private'
            private.mkdir(mode=0o700)
            target = root / 'sentinel'
            target.write_text('preserve')
            (private / '.preparation.lock').symlink_to(target)
            with self.assertRaises(OSError):
                acquire_preparation_lock(private)
            self.assertEqual(target.read_text(), 'preserve')


if __name__ == '__main__':
    unittest.main()
