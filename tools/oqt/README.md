# oqt: experimental standalone workspace coding

From this checkout's repository root, select a project **outside this launcher's
own source tree** (the installed launcher is not changed by source work):

```sh
./oqt --workdir /path/to/project
# Reopen (or join, from a second terminal) the latest Session for that workspace:
./oqt --continue --workdir /path/to/project
```

Real coding is the default. `--demo` runs the synthetic fixture demo instead
(fake model/history, no credentials); the two modes never share Sessions.

This opens upstream Pi's experimental Durable TUI. It uses your owning Pi
model defaults and catalog. Your credentials are brokered by the guard, never
copied. The model gets upstream `read`, `write`, `edit` and `bash`, which only
ever run on confined hands, plus `request_access` for anything outside the
workspace. There is no stock local-tool fallback. `--owner-profile DIR` selects
another owned Pi agent directory. Otherwise `OQT_OWNER_AGENT_DIR`,
`PI_CODING_AGENT_DIR` and then `~/.pi/agent` are used. Nothing is installed,
and no service, default or PATH entry changes.

## Architecture

```
launcher (TUI) ──unix──▶ host (upstream server + guard) ──spawns──▶ brain (confined session worker)
     │                       │  guard: ledger carriers, hands, owner broker      │
     └── operator socket ───▶│                                                   ├─ per-session tool socket
                             └── hands (bwrap) ◀──── epoch-fenced transport ─────┘
```

- **Host**: a detached process per workspace profile. Launchers attach to a
  running host or start one under a lock. It retires when no client or worker
  remains.
- **Guard**: owns the permission ledger, the confined hands, the owner
  credential broker and the operator socket. TUI decisions go straight to the
  guard, so no brain can approve its own requests.
- **Brain**: each Session worker runs inside `oqt_brain`
  (bwrap/seccomp/Landlock) with explicit binds only: private source snapshot,
  pinned checkout, its session store and its own sockets. Your `auth.json`,
  `~/.ssh`, the ledger and guard state are not visible. The brain has network
  for model providers.
- **Hands**: guard-owned `RemoteExecutionEnv` daemons in bwrap/seccomp/Landlock
  sandboxes. They see only the workspace plus approved resources, with no
  network unless granted. Every generation has a fresh epoch, and tool calls are
  pinned to the generation admitted for them. Completed, failed or revoked calls
  retire their executor, descendants included, before delivery.

## Operator commands and durable receipts

`!command` runs in confined hands without involving the model or asking approval
for the operator's own intent. Hard denies and resource limits still apply;
operator shells have a 30-second hard timeout and do not create background jobs.
The guard journals before effects, retires descendants, rechecks authority and
only returns success after the owning worker acknowledges durable result
admission. Output becomes a passive native entry, **not a synthetic prompt**;
it cannot initiate inference. Two clients and a reopened client read the same
Session-owned result. There is no client-local context queue or prompt-prefix
fallback.

`!!command` displays output only to the operator. The brain receives an opaque
completion envelope with empty command/output; the native store records only
receipt identity/hash/status. Private output is deliberately not recoverable
from Session history.

Use `/shell-results` to inspect the selected Session's durable results and
protected interrupted invocation IDs. Admission does not mean context placement
or model delivery: a busy conversation queues the passive entry. Explicitly
cancelled writes are not automatically resubmitted. No receipt permits a shell
retry, restores grants or promises exactly-once external effects. A missing ACK
is an uncertain outcome until the exact sealed receipt reconciles; inspect
before retrying. `/permissions ack` records inspection, never replays work.

The initial bounded contract allows 128 operator invocations/results per Session
(including uncertain ones); open a new Session at capacity. Publication JSON is
below 28 KiB, with explicit tail truncation. Oversized commands and exhausted
capacity fail before effects. The permission store holds only sealed effect
metadata; output belongs to native worker transactions. Hosted canonical
history remains oqto-log. See [ADR-0064](../../docs/adr/0064-worker-owned-operator-result-receipts.md).

Source-only gates (disposable data; no installation or ordinary profiles):

```sh
just test-oqt-operator-context /path/to/pinned/pi-source
just test-oqt-bang
just test-oqt-operator-recovery
```

These qualify the local pinned composition, not remote Account/MFA, mobile
adapters, power-loss durability or cross-machine ownership transfer. Preparation
and asset generation share a private process lock so concurrent cold clients
cannot read an incomplete source snapshot. The lock never applies to interactive
runtime execution.

## Permissions

Reads inside the workspace never prompt. Edits and shell commands show a
menu (Enter = Yes, `1`-`4` pick directly, Esc = No):

1. **Yes** (this call)
2. **Yes, and don't ask again this session** — for shell this covers the
   command prefix (`npm test …`); compound commands (`&&`, `|`, `$(…)`,
   redirects) can only be covered by "all shell commands"
3. **Yes, and always allow in this repo** — remembered by the guard in the
   private profile (never agent-writable); can't override a deny
4. **No** — the agent is told

Shell commands are classified by the trusted guard, never by the agent.
Wrappers that run other commands (`bash`, `env`, `sudo`, `xargs`, …) never get
prefix rules. `request_access` (outside paths/network) offers Session (1 h) or No.

Your own commands: `!cmd` runs a command you type in the confined tools
without a prompt and without the model (you typed it). The model sees its
output only as part of your next message. `!!cmd` keeps the output out of the
model's context. Built-in denies and masks still apply. Limits: 120 s, the last
64 KiB of output, and the tools are replaced afterwards so nothing keeps running.

Commands:
- `/permissions` reopens a dismissed dialog.
- `/permissions revoke` freezes the authority, revokes every grant, confirms the
  hands and their descendants stopped, then installs fresh hands with no grants.
- `/permissions ack` acknowledges interrupted calls once you have inspected
  them.

Completed effects and disclosures cannot be undone.

### Ceilings

oqt has no config of its own (ADR-0060). Approval rules live in the `[guard]`
section of the same sandbox config Oqto uses: `~/.config/oqto/sandbox.toml`
(yours, may loosen or tighten) and `<workspace>/.oqto/sandbox.toml`
(agent-writable, so it **can only narrow**: any `auto` fails the session
closed with the reason shown). `OQT_SANDBOX_CONFIG` overrides the user file.
oqt always confines; `enabled` is the Oqto runner's switch only.

```toml
[guard]
read = "auto"               # workspace reads (default auto)
edit = "prompt"             # write/edit in the workspace (default prompt)
shell = "prompt"            # shell commands (default prompt)
network = "prompt"          # auto | prompt | deny
auto_shell = ["cargo test", "npm test"]   # user file only: these never prompt
outside = "prompt"          # prompt | deny ("auto" is rejected)
paths = ["~/Code"]          # still requestable when outside = "deny"
policy = { "~/Code/shared/**" = "auto", "~/Code/secrets" = "deny" }
```

Path keys are plain paths, optionally ending in `/*` or `/**` (same subtree);
other globs fail closed. `auto` paths are never prompted and are mounted
read-write; `deny` paths are invisible to the tools.
Built-in hard denies always apply: credential and config dirs (`~/.ssh`,
`~/.config`, `~/.pi`, `~/.aws` and others), system roots (`/proc`, `/etc/shadow`
and others), protected runtime roots, the workspace itself, and **anything
containing these**, such as `$HOME` or `/`. Unknown keys or values fail closed.
An explicit deny beats an explicit allow, which beats
everything else. Deny roots are mounted empty inside the hands, so masked
content is invisible to the tools, not just unprompted. A changed ceiling takes
effect when the Session's host starts. Adopting it revokes every grant.

## Crash recovery

- **Brain killed mid-call:** the guard stops the hands, discards in-memory
  permits, records the call as interrupted, revokes every grant and installs
  fresh hands. The TUI reattaches by itself and shows `Interrupted: … effects
  uncertain`. Durable reports the call as interrupted and never replays it. A
  retry needs a fresh decision even if a Session grant was active.
- **Host killed:** hands die with it. On the next launch, the journal marks the
  call interrupted with the same rules, and the dead host's private
  directories are swept.

## Owner credentials

The broker reads bounded, owned, private regular files without following
symlinks. It never runs credential commands. OAuth tokens within 10 minutes of
expiry are renewed with upstream Pi's own file-locked `AuthStorage` and provider
refresh, after writing a private `auth.json.oqt-backup`. Only that provider's
entry is rewritten, so other entries and owning Pi keep working. Brains receive
access tokens and account metadata only; refresh secrets never leave the guard.
A failed renewal fails closed.

## Install

First provide a clean checkout at the qualified Pi pin (see Requirements below)
at `~/.local/share/oqt/pi-source`, or set `OQT_PI_SOURCE` to its existing path.
Do not overwrite an existing checkout to prepare it. Installation is optional
and explicit; from this repository's root:

```sh
just install-oqt  # symlink ~/.local/bin/oqt to this checkout
```

`oqt` on PATH is a symlink to this checkout and still runs everything from
here (builds, private distributions, assets). The pinned Pi source defaults to
`~/.local/share/oqt/pi-source` (override with `OQT_PI_SOURCE`); temporary source
checkouts stop working when cleaned. `just uninstall-oqt` removes the
symlink. Nothing else is installed: no services, no global files.

## Look and feel

The TUI is upstream Pi's client TUI with classic-Pi chrome: your owning theme,
`hideThinkingBlock`, editor behavior, keybindings and custom themes are mirrored
read-only into the private profile, there is no server/session heading, and the
footer shows cwd, model, thinking and cumulative token usage. Still different
from classic Pi: no extension-driven statusline/header, no context-percentage
gauge, and the client loads only the permission plugin.

## Preparation

- **Requirements:** Linux x86_64, bwrap, Node 26, cached Cargo dependencies, and
  the qualified upstream checkout at `cdaf6c227d4718a72b59ccd02152c9614df44cfc`
  (`OQT_PI_SOURCE`). A wrong or dirty pin fails closed.
- **Private derivative:** preparation builds a private source derivative under
  `/var/tmp/q<uid>-<artifact>/`. Upstream code is reused unchanged except for
  the reviewed exact-text patches in `source_patches.py`: confined worker spawn,
  guard-attested worker pid, client reattach after worker loss, connection-bound
  input leases/dispatch fencing, native receipt cancellation, and server error
  logging. A patch fails closed if upstream changes its text.
- **Staged assets:** the exact-source carrier, hands, brain launcher, Landlock
  shim, daemon and BPF are staged privately outside any workspace.
- **Startup:** the session host stays warm for 10 minutes after the last
  client leaves (`OQT_HOST_IDLE_SECONDS`, 0–86400), so the next launch skips
  host start. The runtime runs at normal priority on all CPUs; only first-launch
  builds are bounded. Node's compile cache lives in the runtime root (the
  confined brain gets none: a brain-writable cache could poison other code).
  `OQT_DEBUG=1` writes `<profile>/startup-trace.log` (epoch-ms marks from
  shell, launcher, host and guard; brain marks are in `brain.log`).
- **Logs:** `OQT_DEBUG=1` records brain startup phases. The host log and brain
  log live in the private profile.
- **Network:** `PI_OFFLINE=1` disables upstream's Radius relay, so a session is
  never remotely exposed.

Startup takes about 7–8 s; it was about 3.6 s before the host/brain split.

## Proof

```sh
export OQT_PI_SOURCE="$HOME/.local/share/oqt/pi-source" # exact clean pin above
CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=1 python3 tools/oqt/build-assets.py "$OQT_PI_SOURCE"
CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=1 just test-oqt-core
tools/oqt/typecheck.sh "$OQT_PI_SOURCE"
just test-oqt-hands "$OQT_PI_SOURCE" \
  "$PWD/backend/target/oqt-env/debug/pi-env" \
  "$PWD/backend/target/oqt-prepared/bpf/default-x86_64.bpf" # guard/brain/broker/resources/supervisor
just test-oqt-coding     # real TUI: four tools, exact approvals, reopen
just test-oqt-recovery   # real TUI: brain kill and host kill mid-shell
just test-oqt-access     # real TUI: outside-path request, approval, mount
just test-oqt-multi      # two real TUIs on one host/Session
just test-oqt-ui         # original synthetic demo
just test-oqt-sandbox-config # one config, workspace tighten-only
# All TUI tests use disposable profiles/provider credentials; the shared harness
# ignores ambient owner sandbox policy unless a fixture supplies its own path.
```

Historical evidence on `feat/pi-durable-support` included a live
`openai-codex/gpt-5.6-sol` smoke run and a TUI-approved confined write, with
unchanged owner profile bytes. That is not live acceptance for this integration.
The retained refresh fixture uses the real upstream OAuth code with its token
endpoint stubbed; it is not evidence of live renewal.

### Native input authority

The trusted host binds leases to native coordinator-generated attachment IDs.
A distinct operator connection must confirm each request in the native terminal
(default Deny); headless/no-answer requests expire denied. The requester cannot
approve itself or borrow/release another connection's lease. Leases last at most
one minute. Local prompt/abort/model mutations, detach, expiry, disconnect and
worker replacement reclaim authority. A synchronous generation check fences
native dispatch after asynchronous admission.

Remote input is restricted to prompt/steer/abort. It cannot approve a tool,
change a model, compact, widen a resource grant, or withdraw another source's
queued input. Revocation cancels recorded native submission IDs, including late
receipts; it never reconciles by text/order or retries uncertain submissions.
An uncertain admission stops only the guard-owned worker. Already consumed
input/effects cannot be rolled back. Failed cancellation/stop freezes new input.
These local guard primitives do not implement Account admission or recent MFA;
runner/network control must supply those independently and otherwise deny.

## Limits

- **Operator boundary:** local OS ownership, not Oqto Account authentication.
  Authenticated runner/Web/mobile/SSH integration and deployment are not part
  of this.
- **Trusted processes:** the guard and host are trusted, unconfined code.
- **Brain network:** the brain can reach the network, which it needs for
  providers.
- **Network grants:** these expose the host network, including localhost
  services.
- **Revocation scope:** revocation cannot undo completed effects or erase
  disclosed data.
- **Mount changes:** a resource grant that expires mid-call keeps its mount
  until that call ends (at most 30 s).
- **Session-store visibility:** the brain sees the session store of its own
  workspace profile, which includes sibling Sessions of the same workspace.
- **Upstream type debt:** three pinned OpenAI SDK type mismatches remain in a
  full upstream typecheck. The owned composition and the patched upstream files
  typecheck cleanly.
