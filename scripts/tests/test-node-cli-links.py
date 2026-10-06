#!/usr/bin/env python3
import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "aliases", ROOT / "scripts/dist/materialize-node-cli-links.py"
)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


class Aliases(unittest.TestCase):
    def fixture(self, root):
        (root / "node/bin").mkdir(parents=True)
        node = root / "node/bin/node"
        node.write_text(
            "#!/usr/bin/python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n"
        )
        node.chmod(0o755)
        target = root / "node/lib/node_modules/npm/bin/npm-cli.js"
        target.parent.mkdir(parents=True)
        target.write_text("#!/usr/bin/env node\n// original module location\n")
        alias = root / "node/bin/npm"
        alias.symlink_to("../lib/node_modules/npm/bin/npm-cli.js")
        return alias, target

    def test_real_target_and_adjacent_node_survive_wrong_path_and_spaces(self):
        with tempfile.TemporaryDirectory(prefix="node aliases ") as raw:
            root = Path(raw)
            alias, target = self.fixture(root)
            records = mod.materialize(root)
            self.assertEqual(
                records,
                [
                    {
                        "path": "node/bin/npm",
                        "target": "node/lib/node_modules/npm/bin/npm-cli.js",
                    }
                ],
            )
            self.assertFalse(alias.is_symlink())
            wrong = root / "wrong"
            wrong.mkdir()
            (wrong / "node").write_text("#!/bin/sh\nexit 99\n")
            (wrong / "node").chmod(0o755)
            args = json.loads(
                subprocess.check_output(
                    [str(alias), "argument with spaces"],
                    env=dict(os.environ, PATH=str(wrong) + ":/usr/bin:/bin"),
                    text=True,
                )
            )
            self.assertEqual(Path(args[0]).resolve(), target)
            self.assertEqual(args[1], "argument with spaces")

    def test_entire_set_validated_before_any_alias_changes(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            alias, target = self.fixture(root)
            outside = root.parent / "outside-cli.js"
            bad = root / "node/bin/bad"
            bad.symlink_to(outside)
            with self.assertRaises((ValueError, FileNotFoundError)):
                mod.materialize(root)
            self.assertTrue(alias.is_symlink())
            self.assertTrue(target.is_file())

    def test_unexpected_symlink_location_and_directory_are_rejected(self):
        for location in ["elsewhere", "node/bin/dir"]:
            with tempfile.TemporaryDirectory() as raw:
                root = Path(raw)
                alias, _ = self.fixture(root)
                (root / location).symlink_to(
                    root / "node/lib", target_is_directory=True
                )
                with self.assertRaises(ValueError):
                    mod.materialize(root)
                self.assertTrue(alias.is_symlink())


if __name__ == "__main__":
    unittest.main()
