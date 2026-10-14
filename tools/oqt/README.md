# oqt: experimental standalone workspace coding

Run it from a project directory **outside this launcher's own source tree**:

```sh
/home/wismut/byteowlz/oqto-durable-support/oqt --workdir /path/to/project
# Reopen (or join, from a second terminal) the latest Session for that workspace:
/home/wismut/byteowlz/oqto-durable-support/oqt --continue --workdir /path/to/project

Real coding is the default. `--demo` runs the synthetic fixture demo instead
(fake model/history, no credentials); the two modes never share Sessions.
```

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

## Permissions

| Request | Dialog choices | Effect |
|---|---|---|
| Workspace tool call | Allow once · Allow workspace tools for this Session (60 s) · **Deny** | Exact arguments (once) or the fixed workspace-tools capability (Session) |
| `request_access` path/network | Allow for this Session (1 h) · **Deny** | Exact canonical path (read or write) or host network, mounted on fresh hands; tool calls are still approved one by one |

Commands:
- `/permissions` reopens a dismissed dialog.
- `/permissions revoke` freezes the authority, revokes every grant, confirms the
  hands and their descendants stopped, then installs fresh hands with no grants.
- `/permissions ack` acknowledges interrupted calls once you have inspected
  them.

Completed effects and disclosures cannot be undone.

### Ceilings

The global file `~/.config/oqt/policy.toml` is optional and only read. The
workspace file `<workspace>/.oqt/policy.toml` is also optional. It is
agent-writable, so it **can only narrow**.

```toml
workspace_tools = "ask"     # allow | ask | deny
network = "allow"           # allow | ask | deny
outside_paths = "ask"       # ask | deny ("allow" is rejected)
allow_paths = ["~/Code/shared"]   # global file only: never prompted, mounted read-write
ask_paths = ["~/Code"]      # still requestable when outside_paths = "deny"
deny_paths = ["~/Code/secrets"]   # invisible to the tools, wins over allow/ask
```

Built-in hard denies always apply: credential and config dirs (`~/.ssh`,
`~/.config`, `~/.pi`, `~/.aws` and others), system roots (`/proc`, `/etc/shadow`
and others), protected runtime roots, the workspace itself, and **anything
containing these**, such as `$HOME` or `/`. Unknown keys or values fail closed.
`allow_paths` is honored only in the global file (the workspace file may deny
or ask, never allow); an explicit deny beats an explicit allow, which beats
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
  guard-attested worker pid, client reattach after worker loss, and server error
  logging. A patch fails closed if upstream changes its text.
- **Staged assets:** the exact-source carrier, hands, brain launcher, Landlock
  shim, daemon and BPF are staged privately outside any workspace.
- **Logs:** `OQT_DEBUG=1` records brain startup phases. The host log and brain
  log live in the private profile.
- **Network:** `PI_OFFLINE=1` disables upstream's Radius relay, so a session is
  never remotely exposed.

Startup takes about 7–8 s; it was about 3.6 s before the host/brain split.

## Proof

```sh
just test-oqt-hands /tmp/oqto-pi-service-spike.PVTso2/source \
  /tmp/oqto-pi-env-qualified-target/debug/pi-env \
  backend/target/oqt-prepared/bpf/default-x86_64.bpf   # guard/brain/broker/resources/supervisor
just test-oqt-coding     # real TUI: four tools, exact approvals, reopen
just test-oqt-recovery   # real TUI: brain kill and host kill mid-shell
just test-oqt-access     # real TUI: outside-path request, approval, mount
just test-oqt-multi      # two real TUIs on one host/Session
just test-oqt-ui         # original synthetic demo
just test-adaptive-sandbox /tmp/oqto-pi-service-spike.PVTso2/source
cd backend && cargo test -p oq-permissions --test lifecycle --test ceiling
```

Live acceptance with the owning `openai-codex/gpt-5.6-sol` passed: a smoke run
and a TUI-approved confined write with same-turn completion. Credentials went
through the guard broker into the confined brain, and owner profile bytes
stayed unchanged. The real upstream OAuth refresh path is proven with only the
token endpoint stubbed at `fetch`; a live renewal happens whenever a token
nears expiry.

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
