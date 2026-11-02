# ADR-0072: `oqt` is one executable; verification happens at install time

- Status: accepted
- Date: 2026-10-27
- Supersedes: the launch-time full-content verification introduced with ADR-0066

## Context

The installed `oqt` entry grew from the packaging tools. `~/.local/bin/oqt` was a
shell script that resolved `current`, then ran `launch.py` under Python, which
imported the installer modules, re-hashed every file in the runtime (26,260 files,
892 MB), prepared state, built the child environment, and finally `exec`'d node.

Two problems followed:

1. **Startup cost.** Every launch re-read and hashed the whole runtime. Measured
   at 4.15 s of a ~10 s cold start, on every start, for every user.
2. **The check did not buy what it looked like it bought.** The shell entry and
   `launch.py` (and the verifier module itself) execute *before* any comparison.
   All of them are written by the same user in the same tree, with no privilege
   boundary. Anyone able to modify a file inside `runtimes/<hash>/` can equally
   modify the code that decides whether to check. What the hash did detect was
   accidental damage: a partial install, disk corruption, a stray edit.

The isolation the design wanted — "never execute checkout code" — is provided by
the install itself: an immutable copy, named by the hash of its contents, with
read-only files, a private parent, and a manifest written once at install time.

## Decision

- `~/.local/bin/oqt` is a three-line `sh` script: resolve `current` once, set the
  node compile cache, `exec <runtime>/bin/node <entry>`. Nothing else runs between
  the shell and node.
- `installed-entry.ts` does what `launch.py` did — argument parsing, runtime
  identity, workspace/state overlap policy, profile partitioning, environment
  whitelist — in the language that is already running, with no second runtime on
  the hot path.
- Launch-time integrity checks are O(1): runtime root must be an owned `0500`
  directory, `manifest.json` an owned `0400` regular file that parses as schema 1.
  Content is not hashed at launch.
- Full verification remains available as `oqt verify` (the existing Python
  verifier: `verifier.py verify --runtime <dir>`), for CI, after a suspicious
  event, and in the installer, which always verifies before publishing.
- Verification receipts (ADR-0066) are removed: they were a stopgap for keeping
  the wrong mechanism fast, and a stat-based receipt is weaker than either
  alternative it sat between.
- The Python launcher, the Python state module and their tests are deleted. State
  preparation is ported to `installed-state.ts`; its catalog key is pinned by a
  golden test against the values the Python implementation produced, because a
  different key silently repartitions state and hides existing Sessions.

## Consequences

- Cold start with the editor painted: 6.7 s → 0.93 s. Warm start: 0.90 s → 0.58 s
  (`installed_early_editor_test.py`, n=3).
- Corruption that changes file contents is no longer caught at launch; it is
  caught by `oqt verify` and by the installer. Metadata-level accidents (wrong
  owner, writable file, missing or truncated manifest) still fail closed at launch.
- One language on the launch path (shell → node → TypeScript), so state policy is
  unit-tested (`installed-state.test.mjs`) instead of only through PTY runs.
- `environment` is still built from a whitelist and assigned onto `process.env`
  before any child spawns, so the confined brain, guard and host inherit exactly
  the same environment as before.
- Deleting `launch.py` from the runtime layout is a one-way change: runtimes
  installed before this ADR keep working (they still contain their own
  `launch.py`) but are no longer produced.

## Alternatives rejected

- **Keep hashing at launch, cache by receipt.** What we had: still 6.7 s on a
  fresh state, still no real attacker model, plus a new staleness mode.
- **Root-own or sign the install and verify the signature at launch.** Real
  integrity needs a boundary we do not have; that is a separate design, not a
  reason to hash 892 MB per start.
- **Compile the entry to a Rust binary.** Worth doing later for the last ~20 ms,
  but the win here was deleting the work, not the interpreter: node has to start
  anyway, because the application is TypeScript.
