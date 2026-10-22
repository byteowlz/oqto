#!/usr/bin/env python3
"""Read-only placement inventory and bounded installed-runtime verification benchmark.

No arbitrary command adapter: absent authorized readiness probes fail closed.
JSON goes to stdout; the human summary goes to stderr (or --report).
"""
import argparse
import hashlib
import fcntl
import pty
import re
import uuid
import struct
import termios
import json
import math
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import tempfile
import time

METRICS = ('raw_boot', 'first_editable_frame', 'queued_input_acceptance',
           'verification', 'runner_reachability', 'runner_auth', 'guard_ready',
           'first_noop_execution', 'launch_to_first_sealed_noop')
PLACEMENTS = ('local_process', 'rootless_oci', 'microvm', 'remote_runner',
              'kubernetes', 'docker', 'systemd', 'external_runtime')
TOOLS = ('oqt', 'podman', 'docker', 'dpty', 'hypeman', 'firecracker',
         'cloud-hypervisor', 'qemu-system-x86_64', 'kata-runtime',
         'containerd-shim-kata-v2', 'ctr', 'kubectl', 'oqto-runner', 'oqtoctl')


def percentile(values, fraction):
    """Nearest rank; never silently convert unavailable measurements to zero."""
    if not values:
        return None
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)]


def stats(samples):
    values = [s['duration_ms'] for s in samples if s['status'] == 'ok']
    return {'n': len(values), 'failures': len(samples) - len(values),
            'p50_ms': percentile(values, .5), 'p95_ms': percentile(values, .95)}


def run(argv, timeout=20):
    """One CPU, nice 19, bounded output, isolated process group; never export output."""
    start = time.monotonic_ns()
    def restrict():
        os.nice(19)
        os.sched_setaffinity(0, {min(os.sched_getaffinity(0))})
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        try:
            p = subprocess.Popen(argv, stdout=out, stderr=err, start_new_session=True,
                                 preexec_fn=restrict)
        except OSError:
            return {'status': 'unavailable', 'start_ns': start,
                    'end_ns': time.monotonic_ns(), 'duration_ms': None}, b''
        try:
            p.wait(timeout=timeout)
            status = 'ok' if p.returncode == 0 else 'failed'
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.wait()
            status = 'timeout'
        end = time.monotonic_ns()
        out.seek(0)
        data = out.read(262145)
        if len(data) > 262144:
            status, data = 'output_limit', b''
        return {'status': status, 'exit_code': p.returncode, 'start_ns': start,
                'end_ns': end, 'duration_ms': (end - start) / 1e6,
                'stdout_sha256': hashlib.sha256(data).hexdigest()}, data


def inventory():
    result = {'tools': {}, 'probes': {}, 'kvm': {'exists': Path('/dev/kvm').exists(),
              'read_write': os.access('/dev/kvm', os.R_OK | os.W_OK)},
              'guest_images': 'not_discovered; no private runtime stores scanned'}
    for name in TOOLS:
        path = shutil.which(name)
        entry = {'path': path}
        if path:
            # Help is the mandatory first interaction, even for inventory-only tools.
            record, _ = run([path, '--help'], 15)
            entry['help'] = record
            if name in ('podman', 'docker', 'ctr', 'oqto-runner'):
                record, data = run([path, '--version'], 10)
                entry['version_probe'] = record
                if record['status'] == 'ok':
                    entry['version'] = data.decode('utf8', 'replace').strip()[:200]
        result['tools'][name] = entry
    # Read-only local engine queries, never run/pull/start or contact fleet endpoints.
    probes = {'podman_images': ['podman', 'images', '--format', 'json'],
              'podman_runtimes': ['podman', 'info', '--format', '{{json .Host.OCIRuntimes}}'],
              'docker_images': ['docker', '--host', 'unix:///run/docker.sock',
                                'image', 'ls', '--format', '{{json .}}']}
    for name, argv in probes.items():
        if not shutil.which(argv[0]):
            result['probes'][name] = {'status': 'unavailable'}
            continue
        record, data = run(argv)
        # Export IDs only, not image names, engine config, diagnostics or credentials.
        if record['status'] == 'ok':
            try:
                parsed = json.loads(data) if name != 'docker_images' else [json.loads(x) for x in data.splitlines()]
                if name == 'podman_runtimes':
                    record['runtime_names'] = sorted(parsed or {})
                else:
                    record['image_ids'] = [x.get('Id', x.get('ID')) for x in (parsed or [])]
            except (ValueError, TypeError, AttributeError):
                record['status'] = 'invalid_output'
        result['probes'][name] = record
    return result


def unavailable_matrix():
    reasons = {
        'local_process': 'Installed oqt verification is available; no disposable canonical runner/guard readiness adapter wired into this harness.',
        'rootless_oci': 'PodmanSupervisor exists in source; no prepared disposable Workspace image + authorized runner/guard probe supplied.',
        'microvm': 'No implemented microVM PlacementSupervisor in oqto-placement; need prepared guest kernel/rootfs, supported transport, disposable lifecycle and runner/guard auth probe. KVM and Kata shim alone are insufficient.',
        'remote_runner': 'No explicitly authorized disposable remote endpoint, credentials or lifecycle supplied; no SSH/fleet discovery performed.',
        'kubernetes': 'No implemented Kubernetes PlacementSupervisor or explicitly authorized disposable cluster fixture supplied.',
        'docker': 'Engine inventory is not a Docker PlacementSupervisor; no supported authorized runner/guard benchmark fixture supplied.',
        'systemd': 'No disposable unit authorized; live services must not be restarted.',
        'external_runtime': 'dpty/hypeman presence is not an integrated authorized oqt runner/guard adapter.',
    }
    return {p: {m: {'status': 'unsupported' if m == 'queued_input_acceptance' else 'not_measured',
                    'reason': 'Startup queued input is not implemented.' if m == 'queued_input_acceptance' else reasons[p],
                    'samples': [], 'stats': stats([])} for m in METRICS} for p in PLACEMENTS}


def benchmark(samples=5, timeout=30, collect_inventory=True):
    report = {'schema_version': 1, 'clock': 'time.monotonic_ns',
              'host': {'system': platform.system(), 'release': platform.release(),
                       'machine': platform.machine(), 'python': platform.python_version()},
              'limits': {'cpu_count': 1, 'nice': 19, 'timeout_seconds': timeout},
              'definitions': {'cold': 'first verification invocation in this experiment; OS caches NOT dropped',
                              'warm': 'subsequent independent verification invocations; NOT warm host attach',
                              'verification': 'oqt --runtime-info wall time including launcher and full artifact verification; no Session/profile opened',
                              'excluded': 'LLM/network/provider calls; no boot/readiness inferred from verification'},
              'inventory': inventory() if collect_inventory else {},
              'placements': unavailable_matrix()}
    target = report['placements']['local_process']['verification']
    path = shutil.which('oqt')
    if path:
        identity = None
        for index in range(samples):
            record, data = run([path, '--runtime-info'], timeout)
            record['phase'] = 'cold' if index == 0 else 'warm'
            if record['status'] == 'ok':
                try:
                    parsed = json.loads(data)
                    current = {k: parsed[k] for k in ('artifact', 'runtime', 'source', 'node')}
                    if identity is not None and identity != current:
                        record['status'] = 'artifact_changed'
                    identity = current
                except (ValueError, KeyError, TypeError):
                    record['status'] = 'invalid_output'
            target['samples'].append(record)
        report['artifact'] = identity
        target.update(status='measured' if all(s['status'] == 'ok' for s in target['samples']) else 'incomplete',
                      reason=report['definitions']['verification'], stats=stats(target['samples']))
        target['by_phase'] = {phase: stats([s for s in target['samples'] if s['phase'] == phase])
                              for phase in ('cold', 'warm')}
    return report


PINNED = 'bf11f15f74645fcb2a001bf5cd844bee7c0d8a9199b4399686bcc191a597c31c'


def retire_fixture(runtime, owner, state_root):
    """PIDFD + positive owner/runtime identity; never signal an ambient host."""
    for manifest in state_root.glob('p/*/host.json'):
        info = json.loads(manifest.read_text())
        try:
            descriptor = os.pidfd_open(info['pid'])
        except ProcessLookupError:
            continue
        try:
            environment = Path(f'/proc/{info["pid"]}/environ').read_bytes().split(b'\0')
            command = Path(f'/proc/{info["pid"]}/cmdline').read_bytes()
            if (f'OQT_OWNER_AGENT_DIR={owner}'.encode() not in environment
                    or str(runtime).encode() not in command or b'/host.ts' not in command):
                raise RuntimeError('unproven_fixture_host')
            signal.pidfd_send_signal(descriptor, signal.SIGTERM)
        except FileNotFoundError:
            pass
        finally:
            os.close(descriptor)
    deadline = time.monotonic() + 8
    while list(state_root.glob('p/*/host.json')) and time.monotonic() < deadline:
        time.sleep(.1)
    if list(state_root.glob('p/*/host.json')):
        raise RuntimeError('fixture_host_did_not_retire')
    key = hashlib.sha256(os.fsencode(state_root.resolve())).hexdigest()[:10]
    alias = Path('/var/tmp') / f'qi{os.getuid()}-{key}'
    if alias.is_symlink() and os.readlink(alias) == str(state_root):
        alias.unlink()


def receipt_identity(raw, command):
    """Bind a unique exact command to its durable receipt ID, never row position."""
    clean = re.sub(rb'\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)', b'', raw).decode('utf8', 'replace')
    pattern = r'([0-9a-f-]{36}:operator:[0-9a-f-]{36}): admitted\s+\$ ' + re.escape(command) + r'\s*(?:\n|$)'
    ids = set(re.findall(pattern, clean))
    if len(ids) != 1:
        raise RuntimeError('receipt_identity_binding_failed')
    return ids.pop()


def local_tui(report, pairs=5, timeout=90):
    from coding_fixture import coding_world, snapshot
    from tui_test import Terminal

    class InstalledTerminal(Terminal):
        # Reuse PTY expect/send/close, but NEVER Terminal's checkout entrypoint.
        def __init__(self, executable, args, env, cwd):
            self.started = time.monotonic()
            self.master, slave = pty.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 36, 112, 0, 0))
            self.fixture_policy = tempfile.TemporaryDirectory(prefix='oqt-bench-policy-', dir='/var/tmp')
            def restrict():
                os.nice(19)
                os.sched_setaffinity(0, {0})
            try:
                self.process = subprocess.Popen([str(executable), *args], cwd=cwd, env=env,
                    stdin=slave, stdout=slave, stderr=slave, start_new_session=True, preexec_fn=restrict)
            finally:
                os.close(slave)
            self.raw = bytearray()
            self.deadline = time.monotonic() + timeout

    identity = report.get('artifact')
    if not identity or identity['artifact'] != PINNED:
        raise RuntimeError('installed_runtime_not_pinned_bf11')
    runtime = Path(identity['runtime']).resolve(strict=True)
    executable = runtime / 'oqt'
    if not executable.is_file() or 0 not in os.sched_getaffinity(0):
        raise RuntimeError('pinned_entrypoint_or_cpu0_unavailable')
    matrix = {m: {'status': 'not_measured', 'reason': 'Not a canonical runner integration or raw boot measurement.',
                  'samples': [], 'stats': stats([])} for m in METRICS}
    matrix['queued_input_acceptance'].update(status='unsupported', reason='Startup queued input is not implemented.')
    report['placements']['local_standalone_oqt'] = matrix
    report['local_tui'] = {'pairs_requested': pairs, 'runtime': str(runtime), 'cpu': 0,
        'cold': 'new fake owner/workspace/state and new host per pair; cached artifact, no OS cache drop',
        'warm': 'same fixture, same verified host PID, new --continue PTY client',
        'evidence': 'client-observed monotonic bounds; protected revoke RPC acknowledgement; ! true durable sealed receipt; zero provider calls',
        'failures': []}

    def record(metric, phase, pair, start, end):
        matrix[metric]['samples'].append({'status': 'ok', 'phase': phase, 'pair': pair,
            'start_ns': start, 'end_ns': end, 'duration_ms': (end-start)/1e6})

    for pair in range(pairs):
        with coding_world(lambda request: ('text', 'unexpected-provider-call'), 'oqt-placement-') as (provider, owner, work):
            before = snapshot(owner)
            state_home = work.parent / 'state'
            state_root = state_home / 'oqt/runtimes' / PINNED
            env = {**os.environ, 'XDG_STATE_HOME': str(state_home), 'OQT_HOST_IDLE_SECONDS': '60'}
            host_pid = None
            receipt_ids = set()
            try:
                for phase in ('cold', 'warm'):
                    terminal = None
                    start = time.monotonic_ns()
                    try:
                        args = ['--owner-profile', str(owner), '--workdir', str(work)]
                        if phase == 'warm':
                            args.append('--continue')
                        terminal = InstalledTerminal(executable, args, env, work)
                        terminal.expect('oqt coding')
                        ready_ns = time.monotonic_ns()
                        marker = len(terminal.raw)
                        terminal.send('placement-draft-proof')
                        terminal.expect('placement-draft-proof', marker)
                        record('first_editable_frame', phase, pair, start, time.monotonic_ns())
                        terminal.send('\x15')  # clear draft; never submit to provider
                        manifests = list(state_root.glob('p/*/host.json'))
                        if len(manifests) != 1:
                            raise RuntimeError('fixture_host_manifest_count')
                        pid = json.loads(manifests[0].read_text())['pid']
                        if phase == 'warm' and pid != host_pid:
                            raise RuntimeError('warm_host_not_reused')
                        host_pid = pid
                        status_text = Path(f'/proc/{pid}/status').read_text()
                        allowed = status_text.split('Cpus_allowed_list:')[1].split()[0]
                        nice = int(Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[16])
                        if allowed != '0' or nice != 19:
                            raise RuntimeError('fixture_host_resource_limits')
                        report['local_tui'].setdefault('observations', []).append({
                            'pair': pair, 'phase': phase, 'ready_cue_ns': ready_ns,
                            'host_pid': pid, 'host_cpu_list': allowed, 'host_nice': nice})
                        marker = len(terminal.raw)
                        terminal.send('/permissions revoke\r')
                        terminal.expect('Session grants revoked', marker)
                        record('guard_ready', phase, pair, start, time.monotonic_ns())
                        marker = len(terminal.raw)
                        noop_start = time.monotonic_ns()
                        command = 'true # oqt-placement-' + uuid.uuid4().hex
                        terminal.send('! ' + command + '\r')
                        terminal.expect('durably recorded for this Session', marker)
                        noop_end = time.monotonic_ns()
                        if b'[exit ' in terminal.raw[marker:]:
                            raise RuntimeError('noop_nonzero_exit')
                        marker = len(terminal.raw)
                        terminal.send('/shell-results\r')
                        terminal.expect('$ ' + command, marker)
                        receipt_id = receipt_identity(bytes(terminal.raw[marker:]), command)
                        if receipt_id in receipt_ids:
                            raise RuntimeError('receipt_identity_reused')
                        receipt_ids.add(receipt_id)
                        marker = len(terminal.raw)
                        terminal.send('/permissions revoke\r')
                        terminal.expect('Session grants revoked', marker)
                        marker = len(terminal.raw)
                        terminal.send('/shell-results\r')
                        terminal.expect(receipt_id + ': admitted', marker)
                        terminal.expect('$ ' + command, marker)
                        if receipt_identity(bytes(terminal.raw[marker:]), command) != receipt_id:
                            raise RuntimeError('receipt_identity_changed')
                        for metric, origin in (('first_noop_execution', noop_start), ('launch_to_first_sealed_noop', start)):
                            record(metric, phase, pair, origin, noop_end)
                            matrix[metric]['samples'][-1].update(receipt_id=receipt_id, command=command)
                        if provider.calls:
                            raise RuntimeError('unexpected_provider_call')
                        if snapshot(owner) != before:
                            raise RuntimeError('fixture_owner_changed')
                    except (AssertionError, OSError, RuntimeError) as error:
                        # No raw PTY exception/log export: it may contain paths or data.
                        code = str(error) if isinstance(error, RuntimeError) else type(error).__name__
                        report['local_tui']['failures'].append({'pair': pair, 'phase': phase, 'code': code})
                        for metric in ('first_editable_frame', 'guard_ready', 'first_noop_execution', 'launch_to_first_sealed_noop'):
                            if not any(s['pair'] == pair and s['phase'] == phase for s in matrix[metric]['samples']):
                                matrix[metric]['samples'].append({'status': 'failed', 'phase': phase, 'pair': pair,
                                    'start_ns': start, 'end_ns': time.monotonic_ns(), 'duration_ms': None})
                        break
                    finally:
                        if terminal:
                            terminal.close(f'placement-{pair}-{phase}')
            finally:
                retire_fixture(runtime, owner, state_root)
                report['local_tui'].setdefault('cleanup', []).append({'pair': pair, 'status': 'retired',
                    'method': 'positive fixture owner/runtime PIDFD SIGTERM; host manifest gone; own IPC alias removed'})
                if snapshot(owner) != before:
                    raise RuntimeError('fixture_owner_changed')
    for metric in ('first_editable_frame', 'guard_ready', 'first_noop_execution', 'launch_to_first_sealed_noop'):
        target = matrix[metric]
        target.update(status='measured' if len(target['samples']) == pairs*2 and all(s['status'] == 'ok' for s in target['samples']) else 'incomplete',
                      reason=report['local_tui']['evidence'], stats=stats(target['samples']))
        target['origin'] = 'post-ready command send' if metric == 'first_noop_execution' else 'PTY launch before process spawn'
        target['interpretation'] = ('Post-ready command roundtrip to durable seal/commit acknowledgement; identity validated by two receipt reads.'
            if metric == 'first_noop_execution' else
            'Launch-to-protected-revoke RPC ACK upper bound, including editor/draft and probe overhead.' if metric == 'guard_ready' else
            'Launch-to-durable noop acknowledgement; receipt identity subsequently validated.' if metric == 'launch_to_first_sealed_noop' else
            'Ready cue plus unsubmitted typed draft observed in PTY; not queued input acceptance.')
        target['by_phase'] = {phase: stats([s for s in target['samples'] if s['phase'] == phase]) for phase in ('cold', 'warm')}


def summary(report):
    lines = ['# Oqt placement benchmark', '', 'Standalone oqt PTY results are separate from canonical runner placement readiness.', '']
    for placement, metrics in report['placements'].items():
        lines.append(f'## {placement}')
        for metric, value in metrics.items():
            s = value['stats']
            if 'origin' in value:
                lines.append(f"  Origin: {value['origin']}. {value['interpretation']}")
            if 'by_phase' in value:
                for phase, phase_stats in value['by_phase'].items():
                    lines.append(f"- {metric} ({phase}): {value['status']}; n={phase_stats['n']}, p50={phase_stats['p50_ms']} ms, p95={phase_stats['p95_ms']} ms")
            else:
                lines.append(f"- {metric}: {value['status']}; n={s['n']}, p50={s['p50_ms']} ms, p95={s['p95_ms']} ms")
        lines.append(metrics['guard_ready']['reason'])
        lines.append('')
    return '\n'.join(lines) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--samples', type=int, default=5, help='1..30 verification invocations')
    parser.add_argument('--timeout', type=float, default=30, help='per command seconds, 1..120')
    parser.add_argument('--local-tui', action='store_true', help='authorized disposable pinned installed-runtime PTY cold/warm fixture')
    parser.add_argument('--pairs', type=int, default=5, help='1..10 cold/warm fixture pairs')
    parser.add_argument('--output', type=Path, help='also write stable JSON to this path')
    parser.add_argument('--report', type=Path, help='also write human markdown summary')
    args = parser.parse_args()
    if not 1 <= args.samples <= 30 or not 1 <= args.timeout <= 120:
        parser.error('samples must be 1..30 and timeout 1..120')
    if not 1 <= args.pairs <= 10:
        parser.error('pairs must be 1..10')
    result = benchmark(args.samples, args.timeout)
    if args.local_tui:
        local_tui(result, args.pairs)
    text = json.dumps(result, indent=2, sort_keys=True, allow_nan=False) + '\n'
    if args.output:
        args.output.write_text(text)
    human = summary(result)
    if args.report:
        args.report.write_text(human)
    print(text, end='')
    print(human, file=sys.stderr)


if __name__ == '__main__':
    main()
