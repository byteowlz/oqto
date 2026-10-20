# One-command phone pairing

Workstream `pair-wizard`, `oqto-8x3p.10.7.6`.
[ADR-0069](../adr/0069-one-command-pairing-owner-control.md) defines the owner
management boundary; [personal wire](personal-host-pairing.md) defines enrollment.

**Source feature, not an installed Mac update.** The installed Studio runner lacks
the controller; its isolated HOME/native-source mismatch also blocks activation
until `oqto-8x3p.10.7.5` is reviewed/integrated. Do not run another Runner over its
history, flip HOME, copy profiles or edit/reload its plist as a workaround.

## Interactive

```sh
oqto pair
```

A terminal automatically opens the wizard for missing inputs:

1. Workspaces: existing `~/byteowlz` and `~/work` are preselected; toggle with Space,
   continue with Enter, or add a directory. Paths are canonicalized/deduplicated.
2. Access: session control, read/write Files, terminal as your OS user. Full defaults
   are only selections, not grants. Flag overrides seed the choices; Escape cancels.
3. Network: Tailscale is preferred if available; otherwise local network is
   preselected with an explanation. No provider credential prompt is involved.
4. Review directories, endpoint and permissions. The unrestricted-terminal warning
   is explicit; final confirmation defaults to **No**. Declining creates no state.
5. Discover/reuse the dedicated host Runner and hot-enable personal HTTPS on that
   same process, or start a missing normal-user Runner conservatively. Display the
   private, single-use five-minute QR. Scan it with Oqto on the phone.

`--interactive` forces the wizard. It needs terminal stdin/stderr; piping input
never causes an unexpected prompt. There is no full-screen UI or mouse requirement.

## Equivalent flags

```sh
oqto pair \
  --workspace "$HOME/byteowlz" \
  --workspace "$HOME/work" \
  --access full --network tailscale \
  --non-interactive --yes
```

`full` selects control + writable Files + unrestricted OS-user terminal.
`read-only` selects observation + read-only Files + no terminal. Explicit overrides:
`--sessions none|observe|control`, `--files none|read|write`, `--terminal true|false`.
Terminal access is **not confined** by the selected Workspace directories. Control
of an existing terminal-owned Pi still requires its own fresh confirmation.

`--port` defaults to 39444. Network discovery binds the selected interface IP, not all
interfaces. Tailscale must be running/connected and the phone must reach that
address; LAN mode requires an actual private local address. No firewall rule,
Tailscale ACL, account authentication or privileged setup is silently changed.

TTY complete flags still ask for confirmation unless `--yes` is explicit. Headless
usage requires all workspaces/access/network and `--yes`; incomplete input exits with code 2.
`--yes` cannot bypass confirmation of choices that still require the wizard.

## Output / diagnosis

- stdout: QR only (`--output terminal|json|svg`). Treat **every** QR rendering as a
  secret; never paste it into issue/board/chat or retain it as routine diagnostic.
- stderr: selection prompts, scope summary, warnings and errors. JSON runtime and
  semantic usage failures emit `{ok:false,error:{code,message}}` on stdout; JSON
  cancellation emits `{ok:true,cancelled:true}`. Parser syntax errors use normal
  clap stderr/exit code 2. Broken pipe is benign. An uncertain ticket response is not
  automatically replayed.
- `oqto pair --status`: non-secret schema1 JSON with PID, listener `active`,
  bound plan and authority state path. It never starts or configures a Runner.
- `--runner-home PATH`: explicitly selects an already-running controller profile;
  it is not permission to rewrite HOME or launch another profile.

The owner authority location is `status.state`. Low-level `personal devices` and
`personal revoke DEVICE_ID` accept `--state <that-location>`; the old commands remain
administration over the same grant gate. The QR does not carry filesystem roots.

Private `pairing/listener.json` holds transport configuration only. Existing
unknown config/identities, symlinks, unavailable owner endpoint, an occupied port,
source mismatch or a different active plan fail closed. Roots/network changes do
not silently widen existing Devices or disrupt unrelated sessions. This version
requires the same configured plan for subsequent tickets. The public entrypoint dispatches the same shared implementation before server
configuration/log initialization; it does not start an API backend. Bootstrap
resolves the matching sibling `oqto-runner` from the installed artifact, never
ambient PATH or recursively `oqto`. Both binaries must be installed together.
The lower-level `oqto-runner pair` uses that same implementation for administration.
No managed-service upgrade/migration/reconfiguration command or PATH installation
is claimed here.

Fresh bootstrap uses a private primary socket and log inside `pairing`,
not the backend's usual socket. Existing user config must already select a
single-user/non-provisioning Runner and is not modified. If absent, a known minimal
private config is created. The background process remains after CLI exit; it is
not a newly installed launchd/systemd login service. A registered old/stopped Mac
service is not automatically replaced or loaded; update/activation is a separate
authorized operation, preserving its existing jobs/configuration.

## Proof

```sh
just test-pair-wizard
```

Strict default all-target Clippy, formatting, loopback process tests and a real
PTY complete-wizard/decline proof. Rust tests exercise canonical roots, flags and
headless failures, no-follow/private path and lock rules, same-PID hot enable,
no widening, stable identity and restored listener after restart, occupied-port
and native-HOME refusal, plus a slow owner peer not pausing the primary socket.

Optional **explicit ephemeral private-LAN fixture**, Linux only (pidfd cleanup):

```sh
python3 scripts/tests/test-runner-pair-wizard.py \
  --binary /absolute/path/to/oqto-runner --bootstrap-lan
```

This uses empty temporary workspaces/HOME, clears inherited credentials, and stubs
only the fixture's process census so real user Runners are not attached. It proves
actual flag-driven fresh CLI bootstrap, repeated same-PID pairing and stable
identity, not actual production discovery, phone enrollment, Mac service operation
or tailnet reachability. QR bytes are captured privately and never printed/stored.

Source checkpoint qualification: runner/authority/wire all-target serial 242 passed,
4 ignored (nested subprocess results not double-counted); new pair suites 8 passed;
strict Clippy has 0 code warnings. Existing `proc-macro-error2 2.0.1` future-incompatibility
notice is tracked `oqto-aavh`: transitive dependency upgrade is outside this slice,
not silenced or claimed resolved. Real Mac source-binding/phone/current-native,
production process census, service/PATH installation and broader lifecycle/security
acceptance remain open. Native Mac strict Clippy and all 8 pairing cases now pass,
as does the real PTY full-wizard/decline proof. Initial exit69/Xcode-license failure
was resolved by the user; the subsequent fixture-only `/tmp` symlink failure was
fixed by canonicalizing its temporary directory, not weakening production guards.
Both original failures are retained. This is disposable source qualification,
not installed-service or phone acceptance. No existing Mac service/configuration/
native history was changed.
