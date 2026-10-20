# Pi extension reconciliation and startup readiness

Workstream: `pi-extension-startup` / `oqto-g5x2.1`.

## Managed assets are not identified by directory name alone

`scripts/dist/pi-default-extensions.txt` remains the single default list.
`pi-extension-supersessions.json` records reviewed vendor index SHA256 identities,
including the observed `oqto-todos` and `pi-oqto-todos` assets superseded by
`pi-todolist`. Its source revision and the historical recovery evidence are
recorded in the policy. Refresh identities only from reviewed vendor source or an
explicit, retained recovery receipt; never learn trust from a user's directory.

`managed-pi-extensions.py` is the shared reconciler:

- Setup/upgrade sync passes its trusted code and policy to the home owner's
  isolated Python interpreter. It does not require that principal to traverse
  the operator's private checkout.
- `oqto-usermgr` embeds the same files and invokes them through `runuser` as the
  target principal. Both personal and shared-principal provisioning use this
  path. The required system Python is `/usr/bin/python3` for usermgr.
- Recognized retired indexes are moved outside autoload to
  `~/.pi/agent/extension-backups/managed-<uuid>/`. Whole directories, including
  any auxiliary data, retain their bytes, modes and owners through rename.
- Unknown retired indexes and unrelated extension directories remain active and
  untouched. Unknown modifications to a canonical index fail reconciliation
  before any retirement. They are not silently overwritten or disabled.
- All required source entries are validated before mutation. Replacements are
  completely staged outside autoload first. Copy, archive or activation failure
  restores previously moved paths; successful changes have a JSON receipt.
- An unchanged second run makes no archive or file-content/mtime changes.
  Calls serialize on a per-home lock.
- Symlinked managed parent directories and a mismatched effective UID fail
  closed. Sync no longer guesses that root-owned user paths are deployment
  artifacts and automatically changes ownership. Such damage requires a
  separate, explicit ownership repair.

The policy recognizes an exact managed **index**, not arbitrary user-owned
project/package code. Explicit extension paths, project resources and packages
are not rewritten by this migration. Their effective registrations must still
qualify; unknown duplicate registrations fail rather than being blanket-disabled.
Settings, Todo configuration, workspace Todo storage and native session JSONL
are not migration targets. Within a recognized managed directory, old auxiliary
files are retained in its archive rather than merged into new vendor code.

Rollback is owner-executed: stop a *new* startup attempt, compare the receipt and
active replacement, preserve any subsequently changed replacement separately,
then rename the archived asset back to its original path. Never overwrite a new
user change during rollback. Restoring an obsolete duplicate intentionally
restores the original failing configuration; rollback is preservation, not proof
of chat readiness. Crash-time recovery and fleet-wide activation are separate
acceptance, not inferred from exception rollback tests.

## Runtime selection is not readiness

Config loading selects one runtime without executing Pi. An explicit path is
exclusive. Once selected, a rejected runtime is never replaced by an unrelated
binary or retried as a supposedly healthy fallback. Metadata-only history
maintenance and independently granted Files/terminal/existing-runtime observation
do not require or launch a second Pi.

For an explicit **native-only** preflight:

```sh
oqto-runner --config /absolute/config.toml check-pi --workdir /actual/workspace
```

Run as the intended principal, with that principal's HOME/XDG/environment. It
emits schema-1 JSON with `scope: "native_rpc_only"`, effective UID, workdir,
selected runtime, `ok` and an optional error; exit is nonzero for a rejected
probe. The probe requests a correlated, typed `get_state` with a native session
identity, a five-second deadline and bounded stdout. Survival or unsolicited
events are insufficient. It sends no prompt, uses `--no-session`, and kills/reaps
only the child it started. Arbitrary native stderr is not forwarded because it
can contain credentials. Parser/identity admission failures use normal CLI
stderr errors. This check is **not** provider authentication, a tool execution
proof, a production-sandbox check, or authorization to attach a private session.

Actual runner-owned Session creation separately awaits real `get_state` through
its existing command/response path under the actual session environment, cwd and
sandbox. First stdout/event arrival cannot mark it ready. Requested startup model
selection must succeed and match a subsequent state; failure does not silently
continue with a default model.

Early exit and broken stdin mark the process unavailable, resolve pending calls,
and remove its process-generation-fenced slot and aliases. Async startup
cancellation retains the per-routing-key creation permit until owned-child
cleanup finishes; concurrent creation cannot return optimistic success or spawn
a duplicate. Cleanup never removes another incarnation's slot. Close shares
that serialization and checks the original process token before affecting it.
No prompt or uncertain effect is automatically replayed.

Exit diagnostics classify known tool conflicts and sandbox/module failures.
They include the account UID/workdir and safe extension suffixes where available,
not raw credential-bearing stderr. These changes affect owned process lifecycle,
not adoption or termination of terminal-owned runtimes.

`oqto-log` remains durable history authority; Pi remains the only native JSONL
writer. Platform/external IDs and native graph/head facts are unchanged. A
removed dead in-memory slot is not deleted durable history. Reconnect must still
resolve the canonical platform/native mapping and resume from durable authority;
no text/order/count reconciliation is introduced.

## Reproducible gates

```sh
just test-managed-pi-extensions
just test-pi-startup-readiness
just test-pi-extension-supersession /absolute/pi /reviewed/extension-source /new/evidence-dir
```

The native gate uses the exact observed retired index (`30dc9d…`) outside every
production manifest. It reproduces actual autoload failure; migrates twice;
keeps unknown personal, trusted-project and package fixtures; verifies unique
tools in the real loopback provider request, serialized model/thinking state and
a real synthetic turn; cold-restores the same Pi-owned native ID/history; then
proves an unknown project duplicate fails closed without deleting its code.
All HOME/XDG, settings, models, credentials and native session files are synthetic.
Background naming requests may legitimately declare no tools; the actual agent
request must expose the complete required fixture tool set without duplicates.

These gates do **not** authorize deployment or prove acceptance for an existing
user's production session. Before closing the incident, verify each intended
personal/shared UID with its real workdir, effective extension closure and
production-equivalent sandbox, then verify the actual Elox durable-session
reconnect/resume and authenticated WebUI. A healthy listener/native-only probe or
a clean temporary HOME is insufficient. Preserve existing sessions, grant
boundaries and configuration; never blanket-restart the fleet.
