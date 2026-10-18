"""Run with python3 -m unittest discover -s scripts/lint -p test_rust_test_context.py."""
import importlib.util
import pathlib
import tempfile
import unittest
import subprocess
import sys

from rust_test_context import TestContext


class TestProvenance(unittest.TestCase):
    def fixture(self, files):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = pathlib.Path(tmp.name)
        (root / 'Cargo.toml').write_text('[package]\nname="canary"\nversion="0.1.0"\n')
        for name, text in files.items():
            p = root / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
        return root, TestContext(root, list(files))

    def test_external_not_filename_and_transitive(self):
        root, ctx = self.fixture({
            'src/lib.rs': '#[cfg(test)]\nmod helpers;\nmod tests;\n',
            'src/helpers.rs': 'mod nested;\nfn f() { None::<()>.unwrap(); }',
            'src/helpers/nested.rs': 'fn f() { None::<()>.unwrap(); }',
            'src/tests.rs': 'fn f() { None::<()>.unwrap(); }',
        })
        self.assertTrue(ctx.contains('src/helpers.rs', 20))
        self.assertTrue(ctx.contains('src/helpers/nested.rs', 5))
        self.assertFalse(ctx.contains('src/tests.rs', 5))

    def test_item_bounds_comments_strings_and_attributes(self):
        text = '''const S: &str = "#[cfg(test)]";
#[cfg(test)]
/* comment */
#[allow(dead_code)]
fn helper() { None::<()>.unwrap(); }
fn production() { None::<()>.unwrap(); }
'''
        _, ctx = self.fixture({'src/lib.rs': text})
        self.assertTrue(ctx.contains('src/lib.rs', text.index('None')))
        self.assertFalse(ctx.contains('src/lib.rs', text.rindex('None')))

    def test_path_inline_and_shared_production(self):
        _, ctx = self.fixture({
            'src/lib.rs': '''#[cfg(test)]
#[path = "shared.rs"] mod helper;
#[path = "shared.rs"] mod production;
mod outer { #[cfg(test)] mod helper; }
''',
            'src/shared.rs': 'fn f() {}',
            'src/outer/helper.rs': 'fn f() {}',
        })
        self.assertFalse(ctx.contains('src/shared.rs', 3))
        self.assertTrue(ctx.contains('src/outer/helper.rs', 3))

    def test_inner_cfg_scope(self):
        text = 'mod helper { #![cfg(test)] fn f() { None::<()>.unwrap(); } }\nfn live() { None::<()>.unwrap(); }'
        _, ctx = self.fixture({'src/lib.rs': text})
        self.assertTrue(ctx.contains('src/lib.rs', text.index('None')))
        self.assertFalse(ctx.contains('src/lib.rs', text.rindex('None')))

    def test_unknown_cfg_is_production(self):
        _, ctx = self.fixture({
            'src/lib.rs': '#[cfg(any(test, feature="live"))]\nmod helper;',
            'src/helper.rs': 'fn f() {}',
        })
        self.assertFalse(ctx.contains('src/helper.rs', 3))

    def test_cargo_integration_roots_and_production_reuse(self):
        _, ctx = self.fixture({
            'src/lib.rs': '#[path="../tests/shared.rs"] mod production;',
            'tests/suite.rs': 'mod helper; fn f() {}',
            'tests/helper.rs': 'fn f() {}',
            'tests/shared.rs': 'fn f() {}',
            'tests/nested/main.rs': 'fn f() {}',
        })
        self.assertTrue(ctx.contains('tests/suite.rs', 3))
        self.assertTrue(ctx.contains('tests/helper.rs', 3))
        self.assertTrue(ctx.contains('tests/nested/main.rs', 3))
        self.assertFalse(ctx.contains('tests/shared.rs', 3))

    def test_explicit_test_and_production_entries_override_directory_names(self):
        _, ctx = self.fixture({
            'Cargo.toml': '[package]\nname="canary"\nversion="0.1.0"\nautotests=false\n[[test]]\npath="fixtures/proof.rs"\nname="proof"\n[[bin]]\nname="live"\npath="fixtures/live.rs"\n',
            'fixtures/proof.rs': 'fn f() {}',
            'fixtures/live.rs': 'fn f() {}',
            'tests/not_enabled.rs': 'fn f() {}',
        })
        self.assertTrue(ctx.contains('fixtures/proof.rs', 3))
        self.assertFalse(ctx.contains('fixtures/live.rs', 3))
        self.assertFalse(ctx.contains('tests/not_enabled.rs', 3))

    def test_production_canary_report(self):
        spec = importlib.util.spec_from_file_location(
            'guardrail_report', pathlib.Path(__file__).with_name('rust-ai-guardrails-report.py'))
        report = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(report)
        root, _ = self.fixture({
            'src/lib.rs': '#[cfg(test)] mod helper;\nfn live() { None::<()>.unwrap(); }',
            'src/helper.rs': 'fn test_helper() { None::<()>.unwrap(); }',
        })
        old_root = report.ROOT
        self.addCleanup(setattr, report, 'ROOT', old_root)
        report.ROOT = root
        total, rules, files = report._scan([str(root / 'src')], True)
        self.assertEqual(total, 1)
        self.assertEqual(rules['rust-no-unwrap-expect'], 1)
        self.assertEqual(len(files), 1)
        canary = subprocess.run([
            sys.executable, str(pathlib.Path(__file__).with_name('rust-ai-guardrails-report.py')),
            '--paths', str(root / 'src'), '--exclude-cfg-test', '--check',
        ], capture_output=True, text=True)
        self.assertEqual(canary.returncode, 1, canary.stderr)
        self.assertIn('TOTAL\t1', canary.stdout)


if __name__ == '__main__':
    unittest.main()
