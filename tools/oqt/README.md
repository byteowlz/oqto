# oqt: experimental standalone workspace coding

Use it from a project directory **outside this launcher’s own trusted source tree**:

```sh
/home/wismut/byteowlz/oqto-durable-support/oqt --coding --workdir /path/to/project
# Reopen the latest native Session for the same workspace/runtime artifact:
/home/wismut/byteowlz/oqto-durable-support/oqt --coding --continue --workdir /path/to/project
```

This opens upstream Pi’s actual experimental Durable TUI with your owning Pi
model defaults/catalog and **read-only** credentials. It enables upstream
`read`, `write`, `edit`, and `bash`, exclusively through confined hands. There is
no stock local-tool fallback. `--owner-profile DIR` selects another owned Pi
agent directory; otherwise `OQT_OWNER_AGENT_DIR`, `PI_CODING_AGENT_DIR`, then
`~/.pi/agent` are considered. No installed profile, commands, services or defaults
are changed.

## Permissions and limits

- **Allow once** approves the displayed arguments for that logical invocation.
- **Allow workspace tools for this Session (60 seconds)** is broader: it covers
  the fixed workspace-tools capability for this public Session, not just the
  displayed command. It cannot expand filesystem/network policy.
- **Deny** is the default. `/permissions` revisits a dismissed pending dialog.
- `/permissions revoke` freezes admission/delivery, revokes overlays, confirms
  executor/descendant exit, then installs fresh hands. Already completed effects
  or disclosures cannot be undone.
- Tool invocations are sequential and bounded to **30 seconds**, or the remaining
  protected lease minus a stop margin, whichever is shorter. Completed and failed
  invocations retire their executor before delivery: detached writers cannot
  survive a completed call. Background servers/jobs are intentionally unsupported.
- Workspace files are read/write; tool network is isolated. Outside files,
  credentials, runtime/source, ledger, and control sockets remain unavailable.
  Approval does **not** request or grant outside-workspace resources.
- Synchronous progress/details/diagnostics are withheld until delivery checks.
  Output beyond the 1 MiB buffer fails closed. Interrupted effects are uncertain;
  inspect before retrying. Native coding tasks use `replay: "unsafe"`.
- No skills, codemode, subagents, arbitrary plugins or `/reload`. Command-valued
  credentials/model configuration are rejected. Unsupported/missing defaults
  fail rather than silently choosing another model or execution environment.

The brain is a **reviewed, trusted Node host**, not an OS-confined untrusted plugin
host. Private runtime/profile/source and owning credentials are protected from
hands; this is not production brain isolation. Local OS ownership/UID binding is
not Oqto Account authentication. This is usable **experimental local coding**, not
an authenticated Oqto deployment or permission-policy compiler.

## Owning credentials

`readonly-profile.ts` opens bounded, owned regular JSON files without following
final symlinks; credential files must be private. It never runs credential
commands, logs secrets, modifies the owner’s profile, logs out, or rotates OAuth.
Existing API keys and valid OAuth access/account metadata are supported; refresh
secrets are stripped. Expired/near-expiry tokens fail before refresh and before
ambient-key fallback. **Renew/login through owning Pi**; its replacement becomes
visible on the next credential read. There is no automatic refresh broker.

`owner-models.ts` supplies owning defaults/catalog reads and an in-memory catalog
write/delete overlay. Upstream runtime cache updates never write or lock the
owner catalog. Session history and authority live in the separate private runtime
profile; credentials are not copied there.

## Composition and preparation

Linux x86_64, bwrap, Node 26, cached Cargo dependencies and the qualified upstream
checkout at `cdaf6c227d4718a72b59ccd02152c9614df44cfc` are required. Set
`OQT_PI_SOURCE` to its location; wrong/dirty tracked pins fail closed.

Preparation copies the controlled composition into a private source distribution
under `/var/tmp/q<uid>-<artifact>/`. `runSessionWorkerWithHarness` selects
`coding-worker.ts`/`openCodingRuntime`, never the stock unrestricted harness.
Only the controlled TUI package and static permission provider are configured.
Exact-source carrier, hands, Landlock shim, daemon and BPF are privately staged
outside the workspace. Enforcement-helper failure never selects a fallback.

The carrier holds non-deserializable, operation-bound permits. Separate bounded
operator/tool pipes prohibit tool-side decisions/revocation. Invocation and
handoff are reauthorized against the protected ledger; native Durable SQLite is
transcript/task authority, not the permission ledger. Public Session identity is
separate from native conversation/task IDs.

Runtime/profile paths are private and artifact/workspace/mode keyed. Runtime
changes select a new profile; migration across artifacts is not automatic.
Synthetic demo and real coding histories are separate. Socket paths are bounded
below Linux’s 108-byte limit. Unknown/non-private paths and runtime edits are
refused. Preparation caches are performance aids, not authority/signatures.
Build/preparation uses one CPU, nice 19, one Cargo worker and 90-second deadlines;
the interactive process inherits affinity/nice and a 512 MiB V8 heap ceiling,
not an aggregate memory quota. Normal TUI close stops its owned server/workers.

## Proof and remaining qualification

```sh
# Original synthetic model/history demo remains the default:
./oqt --demo --smoke
just test-oqt-ui
# Disposable owner profile + real HTTP provider transport + all four tools + reopen:
just test-oqt-coding
just test-oqt-hands /tmp/oqto-pi-service-spike.PVTso2/source \
  /tmp/oqto-pi-env-qualified-target/debug/pi-env \
  backend/target/oqt-prepared/bpf/default-x86_64.bpf
just test-adaptive-sandbox /tmp/oqto-pi-service-spike.PVTso2/source
# Explicit live owning-provider request, no tools:
./oqt --coding --smoke --workdir /path/to/project
```

Local qualification includes 11 coding/credential/confinement foundations, 27
core/native/replacement tests, three original demo PTY cases, and the real coding
TUI/provider-transport workflow. The composed proof covers active revoke, withheld
secret output, same-turn fresh hands, and detached-descendant stop after normal
completion. A bounded **actual owning `openai-codex/gpt-5.6-sol` request and actual
TUI-approved confined write** passed with same-turn completion; owner auth,
settings, models and catalog bytes stayed unchanged.

Production Account authority, global/workspace ceiling compilation, external
resource expansion, automatic OAuth refresh, simultaneous launcher clients,
in-flight kill/recovery/uncertain-effect reconciliation, browser/mobile/SSH and
deployment remain unqualified. Normal completed-session reopen is not in-flight
crash recovery.

Accepted isolated upstream type debt: three pinned OpenAI response SDK type
mismatches (`prompt_cache_options`, `fast`) remain in a full source check. Owned
composition diagnostics must be clean. The scoped live owning-model path is
runtime-qualified; this is **not** qualification of every provider or a claim
that full upstream TypeScript passes. See ADR-0053 for the production contract.
