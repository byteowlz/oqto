import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import unittest
from rust_guardrail_paths import changed_paths


class ChangedPathsTests(unittest.TestCase):
    def test_all_working_states_and_test_named_production_candidates(self):
        with tempfile.TemporaryDirectory(prefix='oqt-guard-paths-') as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.check_output(['git', '-C', directory, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@invalid', *args], stderr=subprocess.DEVNULL).decode().strip()
            def write(name):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('fn fixture() {}\n')
            git('init', '-q')
            for name in ['modified.rs', 'deleted.rs']:
                write(name)
            git('add', '.')
            git('commit', '-qm', 'base')
            base = git('rev-parse', 'HEAD')
            write('committed.rs'); git('add', '.'); git('commit', '-qm', 'feature')
            (root / 'modified.rs').write_text('fn changed() {}\n')
            (root / 'deleted.rs').unlink()
            write('staged.rs'); git('add', 'staged.rs')
            for name in ['untracked.rs', 'src/tests/production.rs', 'src/production_test.rs', 'space and\nnewline.rs', 'target/generated.rs']:
                write(name)
            self.assertEqual(changed_paths(root, base), sorted([
                'committed.rs', 'modified.rs', 'staged.rs', 'untracked.rs',
                'src/tests/production.rs', 'src/production_test.rs', 'space and\nnewline.rs',
            ]))
            (root / 'escape.rs').symlink_to('/not-a-repository-source')
            with self.assertRaises(ValueError):
                changed_paths(root, base)

    def test_real_gate_rejects_untracked_test_named_production_canary(self):
        source = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory(prefix='oqt-guard-gate-') as directory:
            root = Path(directory)
            lint = root / 'scripts/lint'
            lint.mkdir(parents=True)
            for name in ['rust-ai-guardrails.sh', 'rust-ai-guardrails-report.py', 'rust_test_context.py', 'rust_guardrail_paths.py', 'test_rust_test_context.py']:
                shutil.copy2(source / 'scripts/lint' / name, lint / name)
            # Keep the gate's self-test hook real without recursively invoking
            # this enclosing end-to-end test. Empty discovery exits 5 on 3.14.
            (lint / 'test_rust_guardrail_paths.py').write_text('import unittest\nfrom rust_guardrail_paths import changed_paths\nclass Contract(unittest.TestCase):\n def test_import(self): self.assertTrue(callable(changed_paths))\n')
            shutil.copytree(source / '.ast-grep', root / '.ast-grep')
            (root / 'src/tests').mkdir(parents=True)
            (root / 'Cargo.toml').write_text('[package]\nname="canary"\nversion="0.1.0"\n')
            (root / 'src/lib.rs').write_text('fn baseline() {}\n')
            def git(*args):
                subprocess.run(['git', '-C', directory, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@invalid', *args], check=True, capture_output=True)
            git('init', '-q'); git('add', '.'); git('commit', '-qm', 'baseline')
            (root / 'src/lib.rs').write_text('#[path="tests/production_test.rs"] mod production;\n')
            (root / 'src/tests/production_test.rs').write_text('fn live() { None::<()>.unwrap(); }\n')
            run = subprocess.run(['bash', str(lint / 'rust-ai-guardrails.sh')], env={**os.environ, 'AIGENRIV_BASE_REF': 'HEAD'}, capture_output=True, text=True, timeout=15)
            self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
            self.assertIn('TOTAL\t1', run.stdout)


if __name__ == '__main__':
    unittest.main()
