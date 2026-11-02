#!/usr/bin/env python3
"""Report how far oqt's pinned Pi source is behind upstream main.

Usage: upstream_drift.py [PI_SOURCE] [--no-fetch] [--max-days N] [--json]
Fetches origin (no checkout/working-tree change), then reports commits and days
behind, Durable-package commits, and pinned dependency paths that upstream has
removed. Exits 1 when the pin is older than --max-days (default 14) or depends
on removed upstream paths, so gates fail visibly instead of drifting silently.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
# Upstream locations oqt composes or patches; removal means oqt owns them now.
DEPENDENCIES = [
    'packages/durable', 'packages/tui', 'packages/env', 'packages/ai',
    'packages/client', 'packages/server', 'packages/protocol',
    'packages/coding-agent/src/experimental/client-tui.ts',
    'packages/coding-agent/src/experimental/server.ts',
    'packages/coding-agent/src/experimental/session-worker.ts',
    'packages/coding-agent/src/experimental/source-resolver.ts',
]


def git(source, *args):
    return subprocess.check_output(['git', '-C', str(source), *args], text=True).strip()


def active_branches(source, head_time, days=14):
    """Branches touching oqt dependencies updated within `days` of upstream main."""
    found = []
    refs = git(source, 'for-each-ref', '--format=%(refname:short)%09%(committerdate:unix)%09%(committerdate:short)%09%(subject)',
               'refs/remotes/origin').splitlines()
    for line in refs:
        ref, stamp, date, subject = line.split('\t', 3)
        if ref in ('origin', 'origin/main', 'origin/HEAD') or head_time - int(stamp) > days * 86400:
            continue
        try:
            base = git(source, 'merge-base', 'origin/main', ref)
        except subprocess.CalledProcessError:
            continue  # unrelated history: not a candidate
        touched = git(source, 'diff', '--name-only', base, ref, '--', *DEPENDENCIES)
        if touched:
            found.append({'branch': ref.removeprefix('origin/'), 'updated': date, 'subject': subject,
                          'ahead': int(git(source, 'rev-list', '--count', f'origin/main..{ref}'))})
    return found


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('source', nargs='?', default=os.environ.get('OQT_PI_SOURCE', str(Path.home()/'.local/share/oqt/pi-source')))
    parser.add_argument('--no-fetch', action='store_true')
    parser.add_argument('--max-days', type=int, default=14)
    parser.add_argument('--json', action='store_true')
    options = parser.parse_args()
    source = Path(options.source)
    pin = re.search(r'^PIN = "([0-9a-f]{40})"', (ROOT/'tools/oqt/prepare.py').read_text(), re.M).group(1)
    if not options.no_fetch:
        # All branches: upstream direction often lands on a branch before main.
        subprocess.run(['git', '-C', str(source), 'fetch', '-q', '--prune', 'origin',
                        '+refs/heads/*:refs/remotes/origin/*'], check=True)
    head = git(source, 'rev-parse', 'origin/main')
    pin_time, head_time = int(git(source, 'show', '-s', '--format=%ct', pin)), int(git(source, 'show', '-s', '--format=%ct', head))
    upstream = set(git(source, 'ls-tree', '-r', '--name-only', 'origin/main').splitlines())
    removed = [path for path in DEPENDENCIES
               if path not in upstream and not any(entry.startswith(path + '/') for entry in upstream)]
    report = {
        'pin': pin, 'upstream': head,
        'commits_behind': int(git(source, 'rev-list', '--count', f'{pin}..{head}')),
        'durable_commits_behind': int(git(source, 'rev-list', '--count', f'{pin}..{head}', '--', 'packages/durable')),
        'days_behind': round((head_time - pin_time) / 86400, 1),
        'removed_upstream_dependencies': removed,
        'releases_since_pin': git(source, 'log', '--format=%s', f'{pin}..{head}', '--grep=^Release v').splitlines(),
        'active_branches': active_branches(source, head_time),
    }
    stale = report['days_behind'] > options.max_days or bool(removed)
    if options.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"pin {pin[:8]} -> upstream {head[:8]}: {report['commits_behind']} commits "
              f"({report['durable_commits_behind']} durable), {report['days_behind']} days behind")
        for line in report['releases_since_pin']:
            print(f'  upstream {line}')
        for branch in report['active_branches']:
            print(f"  active branch {branch['branch']} ({branch['updated']}, +{branch['ahead']}): {branch['subject']}")
        for path in removed:
            print(f'  REMOVED upstream, still depended on: {path}')
        print('STALE: plan the upgrade (oqto issue)' if stale else 'ok')
    return 1 if stale else 0


if __name__ == '__main__':
    sys.exit(main())
