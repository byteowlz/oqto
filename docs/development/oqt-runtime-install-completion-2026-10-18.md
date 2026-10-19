# Standalone oqt runtime installation qualification

2026-10-18 · `oqto-dh3g` · local Linux x86_64 developer-only installation,
not Oqto platform deployment (ADR-0066).

Active runtime:
`bf11f15f74645fcb2a001bf5cd844bee7c0d8a9199b4399686bcc191a597c31c`.

`~/.local/bin/oqt` now resolves through `~/.local/share/oqt/current/oqt` to
`~/.local/share/oqt/runtimes/<hash>/oqt`. It no longer executes code in
`oqto_refactor` or `oqto-durable-support`. The prior checkout symlink, its
independent launcher bytes and metadata were captured under
`~/.local/share/oqt/launcher-backups/launcher-2zmy43_o/` before replacement.
No existing host was restarted, no owner credentials/settings were edited,
and no existing histories were migrated.

## Implementation

- `tools/oqt/install-runtime.py`: full-tree content identity, independent copied
  dependencies and patched workspace package resolution, copied Node/native
  assets, private staging, validation, idempotent publication, unique unmanaged
  launcher backups, atomic current and activation-failure rollback.
- `tools/oqt/install-oqt.sh`, `installed-entry.sh`, `installed-launch.py`: explicit
  bounded source packaging and resolved/pinned execution, never a workspace
  symlink or build-on-every-launch. Read-only `--runtime-info` reports identity.
- `tools/oqt/installed-state.py`: persistent XDG state, owner/workspace/mode
  namespaces and checked short IPC aliases. Alias identity includes the canonical
  state namespace, allowing distinct state roots for the same runtime without
  interference. Unsafe directory ownership/modes, symlinks, binding files and
  alias collisions fail closed without overwriting unknown files.
- `tools/oqt/host.ts`: installed default launcher directory joins the protected
  roots. The complete install store and native state remain protected.
- `backend/crates/oq-permissions/examples/oqt_brain.rs`: preserve both canonical
  and distinct absolute lexical addresses of each already-validated subtree.
  The namespace needs the short alias, while the Landlock serialization resolves
  grants canonically. Root/forbidden-target checks are unchanged. Neither parent
  is granted. Canonical-only reproduced ENOENT; lexical-only reproduced EACCES.
- `just install-oqt`, `just test-oqt-install`, README and ADR-0066 make the supported
  developer workflow explicit. Existing canonical sandbox configuration remains
  `~/.config/oqto/sandbox.toml`; no duplicate configuration namespace was created.

Mutable histories/logs/receipts live under
`$XDG_STATE_HOME/oqt/runtimes/<hash>`; private `/var/tmp/qi<uid>-<namespace>` aliases
only provide short IPC addresses. Local history is worker-owned SQLite through
Durable APIs; publication/catalog authority and protected effect ledgers remain
separate. Hosted authority remains `oqto-log`. Existing artifact-scoped histories
remain at their original locations; installing another artifact does not imply
cross-version adoption or a permission/config migration.

## Evidence

All test/build orchestration was bounded to one CPU/nice 19 with
`CARGO_BUILD_JOBS=1`. Interactive user launches retain normal scheduling.

| Gate | Result | Evidence |
| --- | --- | --- |
| `OQT_INSTALL_PREPARED_JSON=... just test-oqt-install` | 18 installer + 8 state/launch tests pass, including actual dependency packaging/imports, source removal independence, mode/content/entry tampering, rollback and preserved unknown data | `/tmp/oqt-installed-final-unit.log` |
| Actual staged confined coding startup **inside this source checkout** | Local fixture provider receives one request and returns coding-ready; native Session is created in disposable persistent state; owner files remain byte-identical | `/tmp/oqt-installed-real-proof.log` |
| Same test **through updated `~/.local/bin/oqt`** | Pass from `oqto_refactor`; only positively identified fixture host retired using pidfd; no real-provider call | `/tmp/oqt-installed-path-proof.log` |
| `just test-oqt-hands PI_SOURCE DAEMON BPF` | 45/45 real confined/native/composed tests pass | `/tmp/oqt-installed-final-hands.log` |
| New native alias regression | Actual short Unix socket and persistent read/write work; readonly catalog cannot be written; parent/sibling/protected paths remain unreadable; protected target aliases reject in both ro/rw | `/tmp/oqt-brain-alias-{red,green,repeat}.log` |
| `just test-oqt-core` | Rust/schema/format/lifecycle/ceiling/receipt/Clippy, Python and 27 Node checks pass | `/tmp/oqt-installed-final-core.log` |
| `just test-oqt-typecheck PI_SOURCE` | Complete private derivative clean | `/tmp/oqt-installed-final-typecheck.log` |
| `just test-oqt-lint PI_SOURCE` | Pinned SDK Biome: 37 files clean; assigned native alias test also clean | `/tmp/oqt-installed-final-lint.log` |
| Touched Rust example | Build, rustfmt and Clippy `-D warnings` clean | `/tmp/oqt-brain-alias-{build,format,clippy}.log` |
| `just lint-rust-ai-guardrails` | TOTAL 0 across 116 selected Rust files | `/tmp/oqt-installed-final-guardrails.log` |
| Activated command | Resolved PATH target matches the qualified runtime; `oqt --runtime-info` and `oqt --help` pass without opening user Sessions | `/tmp/oqt-installed-{activation,info}.json`, `/tmp/oqt-installed-help.log` |
| Shell/Python/diff checks | Syntax, compilation and `git diff --check` pass | Final session commands |

The real integration test is `tools/oqt/installed_runtime_test.py`; explicitly set
`OQT_INSTALLED_TEST_RUNTIME` to the staged hash directory, and optionally
`OQT_INSTALLED_TEST_EXECUTABLE=~/.local/bin/oqt` to exercise the activated command.
Real dependency packaging is explicit `OQT_INSTALL_PREPARED_JSON` opt-in, never
execution selected from an ambient temporary file.

## Boundaries

This is an explicitly approved standalone source-built experiment, not another
production platform setup/deploy engine, a signed release, Account/MFA acceptance,
mobile/macOS qualification or storage/power-loss proof. Runtime verification and
read-only modes do not protect against an unconfined same-UID process intentionally
chmodding/replacing the installation. The guard's confinement remains the agent
boundary. Default `~/.local/bin` activation is the locally qualified entrypoint.
Old versions and two unactivated diagnostic snapshots are retained; no pruning or
live-process intervention was authorized. Independent Node/dependencies/native
assets are bundled, but standard Linux system libraries, Python and bwrap remain
host prerequisites.
