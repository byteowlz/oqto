# Runner hands: opt-in, consent-per-command pilot

Contract: [ADR-0075](../adr/0075-consent-per-invocation-runner-hands-pilot.md).
Tracking: `oqto-49bm.1`. This is not installed or activated by editing the source.

## Reproduce without real Sessions or services

From the repository root, with oqt's compiled filter and shim already built:

```sh
just test-pi-hands "$HOME/.local/share/oqt/pi-source" \
  "$PWD/backend/target/oqt-prepared/bpf/default-x86_64.bpf" \
  "$PWD/backend/target/debug/examples/oqt_shim"
```

This runs:
- Five authority regressions: no execution before consent; immutable invocation key;
  foreign-device refusal; revocation/expiry; restart uncertainty/no replay.
- Actual Runner/peer/owner binaries, signed requests, pinned HTTPS and real bwrap:
  command execution, no duplicate write, no ambient secret or home-file access,
  cancellation before a delayed effect, and rejected revoked peer.
- Pi extension typecheck and lint.
- A synthetic-model Durable conversation invoking the actual `hands_bash` extension
  tool, destination approval through the owner CLI, and real confined output/file proof.

The standard gate uses loopback between separate client/destination processes.
A separate opt-in physical Mac-to-Linux proof is described below. The test launches zero existing Pi
Sessions and never accesses a real profile. `--no-sandbox` in the disposable host
fixture disables its unrelated generic Pi-launch configuration; hands themselves
still explicitly construct and enforce their own sandbox and compiled filter.

## Manual use after explicit setup approval

Use the newly built `backend/target/debug/oqto-runner`, not an installed older binary.
Do not modify/restart an existing Runner to experiment. Use a separate personal state,
config, port, workdir and credential directory. Linux destination only in this slice.

Destination starts the ordinary personal listener with these additional options:

```text
--personal-hands-bpf /absolute/default-x86_64.bpf
```

The existing personal transport supplies TLS and local-owner pairing. Issue a pairing
QR using the destination's existing `personal pair --state ... --output json` flow,
without Files, Session-control or full-user-terminal grants. Treat the QR file as a
single-use secret; transfer it out of band without posting it to chat/logs. Use an
owner-verified endpoint reachable from the source (LAN/tailnet is sufficient).

On the source, create an owned 0700 directory and enroll into a NEW identity file:

```sh
/path/to/new/oqto-runner personal peer enroll \
  --qr /private/ticket.json --identity /private/peers/linux.json --name my-source
```

The source verifies the pinned leaf before sending its proof. Identity files are
0600, create-only and never overwritten. Requests use signatures and fresh nonces.
An uncertain enrollment may have consumed the ticket; inspect destination devices
before issuing another ticket rather than repeatedly creating orphan enrollments.

### Request and approve one command

Source:

```sh
/path/to/new/oqto-runner personal peer request \
  --identity /private/peers/linux.json \
  --request-key unique-call-id --session-label native-session-id \
  --command 'printf hello > result.txt; cat result.txt'
```

Destination owner (local private admin socket only):

```sh
/path/to/new/oqto-runner personal hands-requests --state /private/destination-state
/path/to/new/oqto-runner personal hands-approve INVOCATION_ID \
  --root /absolute/disposable-workdir --state /private/destination-state
```

Inspect authenticated device identity AND exact command before approval. The Session
label is claimed attribution, not independently authenticated platform identity.
Approval selects a workdir locally and authorizes that command once, for 30 seconds.
No general shell permission follows from pairing or Files access.

Source starts/polls the SAME invocation:

```sh
/path/to/new/oqto-runner personal peer run INVOCATION_ID --identity /private/peers/linux.json
/path/to/new/oqto-runner personal peer status INVOCATION_ID --identity /private/peers/linux.json
```

On a lost reply, inspect the same ID/key. Never generate a new invocation merely
because a response was lost. The provider claims execution before spawn and does
not replay a claimed operation, including after destination restart.

Destination can cancel (also denies a pending request):

```sh
/path/to/new/oqto-runner personal hands-cancel INVOCATION_ID --state /private/destination-state
```

The source has `personal peer cancel` for its own invocations. Device-wide
`personal revoke DEVICE_ID --state ...` fences delivery and execution as well.
Cancellation does not undo earlier file changes; cancelled/uncertain receipts must
not be interpreted as proof of no effects.

### Ordinary Pi: explicit-load extension

No settings installation or existing Session import is necessary:

```sh
PI_HANDS_RUNNER=/absolute/path/to/new/oqto-runner \
PI_HANDS_IDENTITY=/private/peers/linux.json \
PI_HANDS_ENVIRONMENT=linux \
pi --extension /absolute/oqto_refactor/tools/oqt/pi-hands/index.ts
```

The model gets `hands_bash({environment:"linux",command:"..."})`. Unknown targets
fail before contacting a Runner. The extension requests consent, reports invocation
identity, runs only after approval, polls results and attempts cancellation on abort.
`--continue` may be supplied to ordinary Pi as usual; native Pi remains history owner.

**This is an additive remote tool, not a sandbox for ordinary Pi.** Local stock tools
and extensions retain their existing authority. Do not expose this host-side private
identity to an oqt confined brain; its guard-owned remote adapter is a separate step.
No popup, reusable grants, general file API, network access or automatic provisioning
is included. Full integration into oqt's installed host2 path remains open.

## Qualification record (2026-10-29)

- Focused HTTPS execution + peer CLI tests: 2 passed.
- Native extension / synthetic Durable Session proof: passed.
- Authority hands tests: 5 passed; complete authority/wire library suites: 32 + 5 passed.
- Runner lib suite: 215 passed, 2 ignored.
- Clippy all-targets with `-D warnings`: passes; focused production guardrails: zero.
- Dependency debt: Cargo reports future-incompatibility in transitive
  `proc-macro-error2 2.0.1`. No compile/Clippy warning was suppressed and no new
  dependency on this macro package was introduced. Updating the shared dependency
  graph is deliberately outside this opt-in capability change.
- Full personal-host suite is NOT green: the 3821-header native catalog test misses
  its 180s deadline in the CPU0/nice19 lane, including a serial rerun (2652 rows at
  184.5s). Tracked as `oqto-b501`; causality not established. The parallel many-shard
  timing failure passed serially. No deadline was loosened or test disabled.

No release, live service change or real Session migration is claimed by this record.

## Shared-tree deployment build qualification (2026-10-30)

Tracking: `oqto-49bm.1.1`. Rechecked the actual shared dirty checkout after security
remediation, not an isolated HEAD with selected files. `uncertain_hands` and all
`HostAction::Hands*` dispatch/authorization arms are present. The earlier report
caught an in-flight snapshot; no missing-method workaround or disabled capability
was needed.

Passed in that shared checkout:

- Locked check of oqto + Runner.
- Locked native debug build/link of all nine deployment binaries: oqto,
  oqto-sandbox, oqto-ssh-proxy, pi-bridge, oqtoctl, oqto-setup, oqto-runner,
  oqto-files, oqto-usermgr. Optimized release/deploy-fast packaging was not run.
- oqto lib: 347 passed / 2 ignored; egress: 34 passed; authority/wire: 32 + 5 passed.
- Combined all-target Clippy with `-D warnings`, touched-crate fmt, full production
  Rust guardrails (50 changed files, zero violations), crate-boundary check.
- `just test-pi-hands`: destination consent, real confinement/HTTPS, peer CLI and
  synthetic Session/extension proof all pass.
- Frontend production build. Regenerated TypeScript was formatted back to the same
  tracked bytes; no generated source changes remain.
- Deploy transaction regressions: 23 passed; provenance: 5 passed; deploy-mode
  regressions passed; local dev dry-run passed.

Evidence logs: `/tmp/oqto-deploy-{shared-check,binaries-debug,shared-lib-tests,
shared-clippy,guardrails,frontend-build,transaction,provenance,mode,dry-run-local,
hands-regressions}.log`.

No artifact was packaged/activated and no service restarted. The dry-run's printed
"activated" line describes its simulated plan, not a real activation. Normal release
builds intentionally reject the still-dirty shared tree; commit the coherent work or
supply a verified prebuilt artifact. Local experimental builds use the explicit
`--dev --host archvm` path. A dev fleet run also requires the separate shared-host
approval flag; this qualification neither grants nor bypasses that approval.

Advisory debt remains visible: Cargo's transitive proc-macro future-compatibility
notice and Vite's >500KB chunk advisory. Neither was suppressed; neither is a compiler
or Clippy failure. Dependency modernization and frontend bundle splitting are outside
this hands compilation repair and do not justify weakening the deployment gates.
The separately tracked native-catalog timing failure `oqto-b501` is not claimed fixed.

## Physical Mac -> Linux qualification (2026-10-30)

A separate arm64 Runner/client was built from an isolated source snapshot on Mac,
not installed into PATH. `personal peer` then connected DIRECTLY to a disposable
Linux listener on the tailnet, with a destination-issued leaf pin and paired key.
SSH staged the single-use ticket and launched the Mac client; workload requests and
results travelled over the Runner HTTPS protocol, not an SSH command tunnel.

```sh
python3 tools/oqt/pi-hands/mac-linux-proof.py \
  --mac-peer /absolute/isolated/mac/build/backend/target/debug/oqto-runner
```

Passed: command pending before owner consent; destination-selected workdir; actual
Linux shell result; same invocation retried without a duplicate file write; revoked
peer rejected. The script shuts down only its own Linux child and removes its own
Mac temporary credential directory. It never reads real Pi credentials/history or
reconfigures existing Runner/Herdr services.

Build/proof artifact retained on Mac: `~/byteowlz/oqto-hands-pilot-f86a4ea8-9ed/`.
The source was tracked HEAD plus the owned hands files, not the shared dirty Mac
checkout. Its isolated Cargo.lock removed one unrelated oqto -> oqto-egress dependency
edge because the copied local lock included that uncommitted manifest change; no
package versions, checksums or git revisions changed. The subsequent build used
`--locked`, one Cargo job and nice 19. No source checkout or user config was edited.

Evidence: `/tmp/hands-physical-mac-linux.log` on Linux and `build.log` in the Mac
artifact directory. This qualifies the native Mac client and physical transport,
NOT a Mac-hosted confined oqt brain, native Mac execution hands, a running native Pi
TUI on Mac, or a persistent deployment. Those remain separate steps.
