# oqt classic import and native forks: local completion

2026-10-18 — `oqto-g7rk.3`, `oqto-g7rk.5`. Completes the agreed source-only
safety → classic import/resume → worker `/tree` and `/branch` sequence. This is
not closure of the broader authenticated Oqto runner integration epic.

Qualified runtime artifact: `b511ed88a7df1a95a820`, private derivative
`/var/tmp/q1000-b511ed88a7`; Pi pin
`cdaf6c227d4718a72b59ccd02152c9614df44cfc`. Gates ran against the working tree;
no release, installation, deployment, broad commit or ordinary-profile edit.
Unrelated work was preserved. Final tracker receipts: `vrun-n67k` (import) and
`vrun-rsht` (forks); both child issues are closed. The parent runner integration
remains in progress under its separate qualification dependency.

## Implementation surface

- `oqt`, `tools/oqt/launcher.ts`, `prepare.py`, `profile_namespace.py`: CLI and
  canonical owner/workspace catalog selection.
- `tools/oqt/classic-{source,client,inspect}.ts`: frozen read-only source, bounded
  discovery/transport and independent headless inspection.
- `tools/oqt/classic-import{,-host,-worker}.ts`: preflight, protected publication,
  exact archive and native typed selected-history materialization.
- `tools/oqt/session-{forks,router,commands}.ts`, `coding-worker.ts`,
  `source_patches.py`: native worker contracts, coherent routing and exact-pin seams.
- `tools/oqt/operator-receipts.ts`, `guard.ts`, plugin `guard-client.ts`/`tui.ts`:
  conversation-pinned v2 results and guard navigation exclusion.
- `tools/oqt/input-leases.ts`: remote navigation denial/local takeover and dispatch
  fencing; native identity-addressed cancellation is in `session-router.ts`.
- `justfile`, `tools/oqt/typecheck.sh` and colocated native/transport/PTY tests:
  reproducible isolated gates. ADR-0064/0065 and the README describe the contracts.

## Implemented contracts

- Read-only explicit-source inspection/discovery and actual CLI/TUI import.
  Strict UTF-8, regular/no-follow file reads, nanosecond before/after identity
  checks, exact source hashes and bounded traversal/upload. Discovery selection
  pins its digest; cancellation imports nothing. Inspection bypasses SDK/cache
  preparation and absent owner profiles. No JSONL repair or writing.
- Actual destination catalogs include canonical owner profile, workspace and mode.
  Two owners using the same workspace cannot attach each other's host/catalog;
  canonical owner aliases retry the same import. No settings/credentials are read
  merely to derive this path identity.
- Exact archive bytes, source IDs, ancestry and inert records remain in worker-owned
  native storage. Projection v2 uses a model-empty head checkpoint followed by
  ordinary typed native message/tool entries with source provenance. The TUI
  actually shows selected history, not just latent model context. Source-keyed
  mappings retain archive and materialized native IDs; the watermark is the final
  actual entry. Hidden custom context remains hidden. Legacy renderers/extensions
  are not activated.
- Complete ordered tool exchanges are validated before import mutation. Unknown
  or unsupported semantics fail explicitly; inactive archived branches remain
  inert. No tasks, historical effects, source configuration or grants activate.
- Protected sibling publication receipts, outside the writable Session directory,
  govern catalog visibility/admission through failure and host restart. Removing a
  pending marker and forging metadata cannot publish an incomplete import.
- Exact retries are idempotent. Source append creates another immutable snapshot;
  the receipt names only an actually published predecessor. Existing local
  continuation is untouched. Rewrites/truncation/changed source identities fail
  closed. Worker import itself does not support an append-in-place compatibility
  path.
- Worker navigation persistently selects a native conversation and coherently
  binds transcript, input and model state. Native visibility/fork rules apply;
  active work and stale view targets are rejected. Failed rebinding freezes new
  admission. Archive-only bookkeeping entries cannot be executable fork points.
- Receipt v2 binds the observed conversation before effects. A two-phase guard
  fence excludes navigation versus queued/live operator work; passive writes
  resolve their recorded destination, not the latest pointer. A bounded
  Session-owned index is not cloned into forks, filters reads by conversation,
  and reconciles lost ACKs across all branches. `!!` remains opaque. The protected
  128-invocation quota is global to the Session.
- Queued-input cleanup is identity-addressed: canonical native submission lookup
  resolves its original conversation. No current-view fallback; passive writes and
  another attachment's submissions cannot be cancelled through this authority.
  Remote input leases do not authorize `/tree`/`branch` mutations. Local navigation
  reclaims the lease and drains recorded queued input; observation does neither.

## Authority, events, identities and reload

Local conversation/history authority is the worker's native SQLite, written only
through Durable APIs. Host files own catalog/publication metadata; the protected
permission ledger owns grants, effects and sealed hashes, never a second output
or Chat timeline. Hosted canonical history remains `oqto-log`, unchanged.

Events are native Chord transcript/model observations plus exact guard/worker
receipt publication/acknowledgment. Classic origin/UUID and source entry IDs are
provenance. Catalog Session, native conversation, native entry, submission, task
and request identities remain distinct. Mappings and callbacks use these IDs;
there is no message reconciliation by text, length or visible order.

On reopen the worker reads the durable selected conversation before admitting
input, reconnects native observers and reconciles every branch's sealed receipts.
Operator recovery does not restore grants or replay commands. Source retry and
publication recovery revalidate protected evidence before exposing the Session.

## Final reproducible gates

All commands used bounded deadlines, one CPU/nice 19 for test/build orchestration,
and `CARGO_BUILD_JOBS=1`. Profiles, providers and stores were disposable. The
HTTP provider was a local fixture; no real-provider acceptance is inferred.

| Gate | Result | Evidence |
| --- | --- | --- |
| `just test-oqt-core` | Rust schema/format/lifecycle/ceiling/receipt/Clippy, Python lock/owner namespace and 27 Node checks pass | `/tmp/oqt-final-core.log` |
| `just test-oqt-import-native PI_SOURCE` | 3 tests; nine semantic positions, actual SQLite, visible/hidden materialization, rollback/reopen, transport/publication/divergence pass | `/tmp/oqt-final-import-native.log` |
| `just test-oqt-forks PI_SOURCE` | 5 native tests, including real Chord clients and exact queued cancellation/lease cleanup | `/tmp/oqt-final-forks-native.log` |
| `just test-oqt-operator-context PI_SOURCE` | 2 tests; branch binding, all-branch recovery, privacy and capacity | `/tmp/oqt-final-receipts-native.log` |
| `just test-oqt-hands PI_SOURCE DAEMON BPF` | 44 confined native/composed tests pass | `/tmp/oqt-final-hands.log` |
| `just test-oqt-import-tui` | Actual >64 KiB CLI, owner isolation/alias retry, visible historical tool result, discovery cancel, quoted-path alternate-leaf TUI resume, actual provider context, reopen, immutable append, original continuation and rewrite denial | `/tmp/oqt-final-import-tui.log` |
| `just test-oqt-forks-tui` | Two real TUIs: transcript/model/input rebinding, stale open picker rejection, branch-pinned shell context and persisted selection on reopen | `/tmp/oqt-final-forks-tui.log` |
| `just test-oqt-bang` | Real `!`/private `!!`, native context/reopen and hard-deny regression pass | `/tmp/oqt-final-bang.log` |
| `just test-oqt-operator-recovery` | Positively fixture/owner-bound pidfd SIGKILL: descendants stop, interrupted effects do not replay, completed receipt survives | `/tmp/oqt-final-operator-recovery.log` |
| `just test-oqt-lint PI_SOURCE` | Pinned SDK Biome: 37 files clean | `/tmp/oqt-final-pinned-biome.log` |
| `just test-oqt-typecheck PI_SOURCE` | Full prepared derivative clean, using a disposable owner | `/tmp/oqt-final-typecheck.log` |
| Diff/shell/Python checks | `git diff --check`, `bash -n`, Python compilation and namespace indentation checks pass | Final session commands |

`PI_SOURCE=/home/wismut/.local/share/oqt/pi-source`;
`DAEMON="$PWD/backend/target/oqt-env/debug/pi-env"`;
`BPF="$PWD/backend/target/oqt-prepared/bpf/default-x86_64.bpf"` (checkout root).
Runtime receipt: `/tmp/oqt-final-runtime.json`. The navigation authority regression
was red before the dispatch policy fix (`/tmp/oqt-navigation-authority-red.log`)
and green afterward (`/tmp/oqt-navigation-authority-green.log`); the native test
also uses the real service ID so a renamed contract cannot silently evade policy.

The TUI checks use distinct fixture responses, not repeated text that could match
an earlier redraw. Tree tests fingerprint runtime inputs across reopen to reject
qualification against drifting artifacts.

## Explicit boundaries

- Source-only local OS/operator qualification, not Account/MFA, hosted history
  translation, SSH/MacBook sync, physical mobile/device or portable SDK acceptance.
- Runtime artifact changes intentionally select a separate experimental catalog;
  no automatic cross-version migration or cross-machine handoff is qualified.
- Source cwd/model/settings never become execution scope or authority. Full raw
  archive retention is not arbitrary extension/renderer or task restoration.
- The stock server redacts stale-worker errors to an `Internal server error`
  banner. The real two-client test proves no model mutation, then a fresh explicit
  selection succeeds. The native test asserts the precise stale-target rejection.
- Process-crash proof is not storage/power-loss proof or external exactly-once
  execution. Provider context limits remain those of the owning model.
- The separate Herdr arrow/Ctrl+C report still needs the user's input trace;
  trusted providers/workspace profiles and startup ideas remain separate work.
- Existing third-party future-Rust compatibility debt remains `oqto-aavh`, not a
  toolchain-upgrade or blanket future-compatibility claim.

See ADR-0065, ADR-0064 and `tools/oqt/README.md`. Earlier safety qualification is
recorded in `oqt-safety-completion-2026-10-17.md`; extension disposition is analysis
only in `pi-extension-durable-port-audit-2026-10-17.md`.
