# Standalone oqt developer runtime snapshots

Status: accepted for the explicitly requested local Linux x86_64 experiment,
2026-10-18; `oqto-dh3g`. This does not qualify a platform release or deployment.

Running a guard from its writable project tree prevents safely developing that
project through the guard. The installed standalone `oqt` therefore executes a
content-addressed, complete snapshot under
`$XDG_DATA_HOME/oqt/runtimes/<sha256>` (default `~/.local/share/oqt`), with an atomic
`current` pointer. The source checkout is an ordinary workspace; editing it does
not edit the active guard. Resolving the launcher once pins existing processes to
their original runtime. Dependencies, patched Pi code, Node and native confinement
assets are copied rather than linked to mutable source/build directories.

This is a **narrow dev-only exception to ADR-0018's single activation engine**:
`just install-oqt` builds the private standalone experiment, not an Oqto release.
It does not invoke or replace setup/deploy, acquire platform tools, change
services, write owner profiles or claim Account authorization. Production
packaging/activation must converge on `oqto-setup` and the canonical release
manifest (ADR-0016/0018); this helper must not grow a parallel platform installer.
A checkout symlink was rejected because it preserves the self-modification
hazard; copying only the entrypoint or retaining external dependency symlinks
would preserve the same hazard less visibly.

Mutable native histories, receipts, logs and private agent profiles live under
`$XDG_STATE_HOME/oqt/runtimes/<sha256>` (default `~/.local/state`). A validated,
owned `/var/tmp/qi<uid>-<prefix>` symlink supplies short Unix-socket addresses;
its target remains persistent. Runtime hashes intentionally partition catalogs;
there is no automatic cross-version migration. Hosted history authority remains
`oqto-log`; local workers alone write native history through Durable APIs.

The installer stages and checks every file/mode before publication, preserves
unmanaged launchers, and activates without retiring hosts. Runtime trees are
read-only/private and verified against their content identity on launch. This is
not signing, privileged installation, power-loss proof, or an OS boundary against
an unconfined process with the same UID. The confinement guard protects the whole
runtime store, current state and installed launcher directory; ancestors and
workspaces overlapping these roots are refused. Existing sandbox policy stays
in `~/.config/oqto/sandbox.toml` and workspace `.oqto/sandbox.toml`; no second config
namespace is introduced.
