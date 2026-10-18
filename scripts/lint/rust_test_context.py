"""Conservative Rust test provenance using ast-grep's Rust parser, not filenames.

Exact cfg(test) and Cargo test entrypoints are recognised; unknown cfg remains production.
Shared modules remain production if reachable through any production declaration.
"""
import collections
import json
import pathlib
import re
import subprocess
import tomllib


def cargo_entrypoints(base):
    """Positive Cargo test roots; a production entry/edge always wins."""
    manifest = base / 'Cargo.toml'
    if not manifest.is_file():
        return set(), set()
    data = tomllib.loads(manifest.read_text())
    package = data.get('package', {})
    tests = set()
    if package.get('autotests', True):
        tests.update((base / 'tests').glob('*.rs'))
        tests.update((base / 'tests').glob('*/main.rs'))
    for item in data.get('test', []):
        if 'path' in item:
            tests.add(base / item['path'])
        elif 'name' in item:
            tests.update([base / 'tests' / (item['name'] + '.rs'), base / 'tests' / item['name'] / 'main.rs'])
    production = {base / data.get('lib', {}).get('path', 'src/lib.rs')}
    if package.get('autobins', True):
        production.add(base / 'src/main.rs')
        production.update((base / 'src/bin').glob('*.rs'))
        production.update((base / 'src/bin').glob('*/main.rs'))
    if package.get('autoexamples', True):
        production.update((base / 'examples').glob('*.rs'))
        production.update((base / 'examples').glob('*/main.rs'))
    for kind, directory in [('bin', 'src/bin'), ('example', 'examples')]:
        for item in data.get(kind, []):
            if 'path' in item:
                production.add(base / item['path'])
            elif 'name' in item:
                production.update([base / directory / (item['name'] + '.rs'), base / directory / item['name'] / 'main.rs'])
                if kind == 'bin':
                    production.add(base / 'src/main.rs')
    build = package.get('build', 'build.rs')
    if isinstance(build, str):
        production.add(base / build)
    return {p.resolve() for p in tests}, {p.resolve() for p in production}

RULE = '''id: rust-context
language: Rust
severity: hint
rule:
  any:
    - kind: attribute_item
    - kind: inner_attribute_item
    - kind: mod_item
    - kind: function_item
    - kind: impl_item
    - kind: struct_item
    - kind: enum_item
    - kind: const_item
    - kind: static_item
    - kind: use_declaration
    - kind: line_comment
    - kind: block_comment
'''


class TestContext:
    def __init__(self, root, paths):
        self.root = pathlib.Path(root).resolve()
        # Discover declarations outside the changed-file set too.
        sources = set()
        for name in paths:
            path = (self.root / name).resolve()
            base = path if path.is_dir() else path.parent
            while base != self.root and base.parent != base and not (base / 'Cargo.toml').exists():
                base = base.parent
            sources.add(base)
        proc = subprocess.run(
            ['ast-grep', 'scan', '--inline-rules', RULE, '--json=compact',
             '--threads', '1', *map(str, sorted(sources))],
            cwd=self.root, text=True, capture_output=True, check=True)
        nodes = collections.defaultdict(list)
        for node in json.loads(proc.stdout):
            nodes[pathlib.Path(node['file']).resolve()].append(node)
        test_roots, production_roots = set(), set()
        for base in sources:
            tests, roots = cargo_entrypoints(base)
            test_roots.update(tests)
            production_roots.update(roots)
        self.ranges = collections.defaultdict(list)
        edges = collections.defaultdict(list)
        incoming = set()
        for path, entries in nodes.items():
            data = bytearray(path.read_bytes())
            entries.sort(key=lambda n: n['range']['byteOffset']['start'])
            # Mask parser-recognised comments, never strings containing // or /*.
            for n in entries:
                if n['text'].startswith(('//', '/*')):
                    r = n['range']['byteOffset']
                    data[r['start']:r['end']] = b' ' * (r['end'] - r['start'])
            attrs = [n for n in entries if n['text'].startswith('#[')]
            modules = []
            for n in entries:
                text = n['text']
                if text.startswith(('#', '//', '/*')):
                    continue
                r = n['range']['byteOffset']
                start = r['start']
                attached = []
                for attr in reversed(attrs):
                    a = attr['range']['byteOffset']
                    if a['end'] <= start and not data[a['end']:start].strip():
                        attached.append(attr['text'])
                        start = a['start']
                if any(re.fullmatch(r'#\[\s*cfg\s*\(\s*test\s*\)\s*\]', a) for a in attached):
                    self.ranges[path].append((start, r['end']))
                match = re.match(r'(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*([;{])', text)
                if match:
                    modules.append((n, match, attached))
            for n in entries:
                if re.fullmatch(r'#!\[\s*cfg\s*\(\s*test\s*\)\s*\]', n['text']):
                    pos = n['range']['byteOffset']['start']
                    enclosing = [m['range']['byteOffset'] for m, mm, _ in modules
                                 if mm[2] == '{' and m['range']['byteOffset']['start'] < pos < m['range']['byteOffset']['end']]
                    region = min(enclosing, key=lambda r: r['end'] - r['start']) if enclosing else {'start': 0, 'end': len(data)}
                    self.ranges[path].append((region['start'], region['end']))
            for n, match, attached in modules:
                if match[2] != ';':
                    continue
                pos = n['range']['byteOffset']['start']
                parent = path.parent if path in test_roots | production_roots or path.name in ('lib.rs', 'main.rs', 'mod.rs') else path.with_suffix('')
                ancestors = [(m['range']['byteOffset']['start'], mm[1]) for m, mm, _ in modules
                             if mm[2] == '{' and m['range']['byteOffset']['start'] < pos < m['range']['byteOffset']['end']]
                for _, name in sorted(ancestors):
                    parent /= name
                explicit = [re.fullmatch(r'#\[\s*path\s*=\s*"([^"]+)"\s*\]', a) for a in attached]
                explicit = [m[1] for m in explicit if m]
                candidates = [path.parent / explicit[0]] if explicit else [parent / (match[1] + '.rs'), parent / match[1] / 'mod.rs']
                for target in candidates:
                    target = target.resolve()
                    if target in nodes:
                        test = any(lo <= pos < hi for lo, hi in self.ranges[path])
                        edges[path].append((target, test))
                        incoming.add(target)
        production = (set(nodes) - incoming - test_roots) | (production_roots & set(nodes))
        production.update(p for p in nodes if p.name in ('lib.rs', 'main.rs') and p not in test_roots)
        todo = list(production)
        while todo:
            for target, test in edges[todo.pop()]:
                if not test and target not in production:
                    production.add(target)
                    todo.append(target)
        # Require a positively established test edge, not merely lack of reachability.
        test_only = test_roots | {target for source in nodes for target, test in edges[source] if test}
        todo = list(test_only)
        while todo:
            for target, _ in edges[todo.pop()]:
                if target not in test_only:
                    test_only.add(target)
                    todo.append(target)
        self.test_files = test_only - production

    def contains(self, name, byte_offset):
        path = (self.root / name).resolve()
        return path in self.test_files or any(lo <= byte_offset < hi for lo, hi in self.ranges[path])
