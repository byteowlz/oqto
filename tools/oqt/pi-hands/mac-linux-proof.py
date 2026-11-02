#!/usr/bin/env python3
"""Opt-in physical Mac peer -> tailnet Linux Runner proof; disposable state only.
SSH stages the ticket and invokes the Mac client, never executes the workload.
Workload requests and results travel directly over pinned HTTPS to Linux.
"""
import argparse
import json
import os
import re
from pathlib import Path
import selectors
import shlex
import socket
import subprocess
import tempfile
import time
import uuid

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--mac', default='mac')
parser.add_argument('--mac-peer', required=True, help='Absolute isolated oqto-runner build on Mac')
parser.add_argument('--address', required=True)
parser.add_argument('--runner', default='backend/target/debug/oqto-runner')
parser.add_argument('--bpf', default='backend/target/oqt-prepared/bpf/default-x86_64.bpf')
parser.add_argument('--shim', default='backend/target/debug/examples/oqt_shim')
a = parser.parse_args()
assert a.mac_peer.startswith('/'), 'Mac peer binary must be absolute'
ssh = ['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10', a.mac]


def remote(args, *, data=None, check=True):
    return subprocess.run([*ssh, shlex.join(args)], input=data, capture_output=True, check=check, timeout=30)


remote_root = None
with tempfile.TemporaryDirectory(prefix='oqto-hands-physical-', dir='/var/tmp') as temp:
    root = Path(temp)
    root.chmod(0o700)
    home, work, state = root/'home', root/'work', root/'state'
    (home/'tmp').mkdir(parents=True)
    work.mkdir()
    config = root/'config.toml'
    config.write_text(f'[local]\nsingle_user=true\nworkspace_dir={json.dumps(str(work))}\n[local.linux_users]\nenabled=false\n[runner]\nrunner_id="physical-hands-fixture"\n')
    reserve = socket.socket()
    reserve.bind((a.address, 0))
    port = reserve.getsockname()[1]
    reserve.close()
    binary = str(Path(a.runner).resolve(strict=True))
    env = {'HOME':str(home), 'PATH':'/usr/bin:/bin', 'USER':os.environ.get('USER','fixture'),
           'XDG_CONFIG_HOME':str(home/'.config'), 'XDG_STATE_HOME':str(home/'.local/state'),
           'TMPDIR':str(home/'tmp'), 'OQTO_SANDBOX_SHIM_BIN':str(Path(a.shim).resolve(strict=True))}
    log = open(root/'runner.log', 'wb')
    child = subprocess.Popen([binary, '--config', str(config), '--no-sandbox',
                              '--personal-listen', f'{a.address}:{port}', '--personal-origin', f'https://{a.address}:{port}',
                              '--personal-state', str(state), '--pair-output', 'json',
                              '--personal-hands-bpf', str(Path(a.bpf).resolve(strict=True))],
                             env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=log)
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ)
            assert selector.select(20), 'destination did not start'
        qr = child.stdout.readline()
        assert qr and json.loads(qr)['purpose']=='oqto.personal-host.v1', 'no pairing payload'
        # A unique owned directory: cleanup never addresses existing Mac state.
        remote_root = remote(['mktemp','-d','/tmp/oqto-hands-peer.XXXXXXXX']).stdout.decode().strip()
        assert re.fullmatch(r'/tmp/oqto-hands-peer\.[A-Za-z0-9]+', remote_root), 'unrecognized remote fixture path'
        remote(['chmod','700',remote_root])
        ticket, identity = f'{remote_root}/ticket.json', f'{remote_root}/identity.json'
        put = f'umask 077; cat > {shlex.quote(ticket)}'
        subprocess.run([*ssh,put],input=qr,check=True,capture_output=True,timeout=20)

        def peer(*args, check=True):
            result = remote(['env',f'HOME={remote_root}',f'XDG_CONFIG_HOME={remote_root}/config',
                             f'XDG_STATE_HOME={remote_root}/state','PATH=/usr/bin:/bin',a.mac_peer,
                             'personal','peer',*args,'--identity',identity],check=check)
            return json.loads(result.stdout) if check else result

        def owner(*args):
            result = subprocess.run([binary,'personal',*args,'--state',str(state)],env=env,
                                    capture_output=True,check=True,timeout=20)
            return json.loads(result.stdout)

        enrolled = peer('enroll','--qr',ticket,'--name','isolated-mac-peer')
        call_key = str(uuid.uuid4())
        command = 'printf linux-remote-ok >> proof.txt; uname -s; cat proof.txt'
        request = peer('request','--request-key',call_key,'--session-label','disposable-mac-session','--command',command)
        invocation = request['id']
        assert request['state']=='pending' and not (work/'proof.txt').exists()
        assert peer('request','--request-key',call_key,'--session-label','disposable-mac-session','--command',command)['id']==invocation
        assert owner('hands-approve',invocation,'--root',str(work))['applied']
        peer('run',invocation)
        peer('run',invocation)  # fresh signed request, same durable operation
        deadline = time.monotonic()+20
        while True:
            receipt = peer('status',invocation)
            if receipt['state']!='running':
                break
            assert time.monotonic()<deadline, 'execution deadline'
            time.sleep(.05)
        assert receipt['state']=='done' and receipt['exit_code']==0, receipt
        assert 'Linux' in receipt['output'] and 'linux-remote-ok' in receipt['output']
        assert (work/'proof.txt').read_text()=='linux-remote-ok', 'duplicate effect'
        owner('revoke',enrolled['device_id'])
        assert peer('status',invocation,check=False).returncode!=0, 'revoked peer still admitted'
        print('PASS physical Mac arm64 peer -> Linux Runner over direct tailnet pinned HTTPS')
        print('  destination consent; real bwrap/seccomp command; same-ID retry writes once; peer revocation')
        print('  no real Pi profile, Session, installed binary, Runner service or Herdr changes')
    except BaseException:
        log.flush()
        print((root/'runner.log').read_text(errors='replace')[-4000:])
        raise
    finally:
        # Signal/wait only our direct child. Never find/kill a pre-existing Runner.
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=5)
        log.close()
        if remote_root:
            remote(['rm','-rf','--',remote_root])
