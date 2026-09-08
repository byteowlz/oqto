# Machine-scoped Pi provider login

Tracked by `oqto-7e9y`; decision: [ADR-0048](../adr/0048-pi-owns-machine-provider-credentials.md).

## Secure runner connection

Use the [Mac runner runbook](macos-runner.md) to register a dedicated runner without changing the personal Linux placement. The current development connection is **loopback mutual TLS over a supervised SSH forward**. Tailscale can replace the private network path, not mutual TLS or Account authorization. Do not expose the runner as a public unauthenticated TCP service.

1. Provision a dedicated runner principal/profile, sandbox policy and managed Pi installation. The initial login adapter requires `local.single_user = true` and `local.linux_users.enabled = false`; shared/multi-user credential profiles are not supported.
2. Issue dedicated server/client certificates and verify the server SAN. Protect the private keys and runner control socket outside all workload-readable paths. The auth worker also runs under the configured sandbox, including egress/pre-exec enforcement; it receives no SSH-agent socket.
3. Bind the runner TLS listener to loopback. Use an independently supervised SSH local forward, or an explicitly firewalled private network. A private network connection is not an Account grant.
4. Register the target with its exact Account-ID allowlist and mutual-TLS endpoint. Missing/incorrect certificates or host identities must fail before an application request succeeds.
5. Enable provider login separately on both ends. Inventory, SSH access and administrator role do not implicitly grant it. Never configure two Accounts against the same credential profile.

Revocation in this operator-managed deployment means removing the backend grant/configuration and restarting it, and stopping the runner/removing its client trust when revoking transport access. Already completed provider authorization is not revoked by removing a machine from inventory; revoke provider tokens at the provider when required. Automated enrollment, short-lived certificate renewal and online revocation distribution are **not** implemented by this feature.

## Explicit configuration

Backend target (add to the existing target; preserve endpoint and unrelated fields):

```toml
provider_login = true
# account_ids must contain exactly one owning Account ID
```

Owning runner configuration:

```toml
[runner.provider_login]
account_id = "ACCOUNT_ID"
node = "/absolute/path/to/node"
worker = "/usr/local/share/oqto/provider-login/worker.mjs"
```

The worker is a manifest-declared immutable asset, staged by `scripts/dist/sync.sh`. On operator-managed Macs an equivalent private installation directory may be used. All paths are chosen by provisioning, never by an API request. The SDK is resolved from the configured Pi executable's actual package, not a second unrelated global installation. Use Node 22+ and a coherent Pi SDK with `ModelRuntime`; Pi 0.85.1 has been tested. Pi 0.85.0's SDK on the development Mac failed to import a missing `pi-server` dependency; a separate managed 0.85.1 installation avoids changing personal Pi.

The agent directory is the owning process's `PI_CODING_AGENT_DIR`, or its `HOME/.pi/agent`. It must already exist and be covered by the sandbox's permitted paths. Grant read-only access to the managed Node/Pi/worker code where required; do not broaden access to transport credentials. Native Pi performs credential writes with its own locking and preserves unrelated entries.

Back up configuration before editing, preview a targeted diff, merge only selected fields, and retain rollback instructions. Never replace a personal `auth.json`, `models.json`, settings file, SSH identity or existing placement to enable this feature. There is no EAVS prerequisite and no provider-entry synchronization in this login operation.

## macOS DNS and sandbox diagnostics

A network-enabled allowlist profile still needs the specific Darwin resolver socket, `/private/var/run/mDNSResponder`. The shipped `macos-host` template grants that endpoint, not all of `/var/run`. Without it, Node HTTPS requests can fail with `ENOTFOUND` even though the same provider flow works outside the sandbox. Granting only `resolv.conf` does not fix this. Explicit socket denial and network-isolated mode still win; regression coverage lives in `oqto-sandbox/tests/macos_launch.rs`.

## Browser and headless behavior

In the original sidebar, an explicitly granted online Machine has a **Providers** button. Choose a provider and **Sign in** or **API key**. Supported Pi interaction types are text, secret, select, manual code, authorization URL, device code and progress. Secret/manual input is private and is cleared after submission. Authentication state is not put in query/history caches or browser persistent storage. Account/deployment changes unmount that state and attempt cancellation.

Use a provider's device flow or supported redirect/code paste-back for remote login. A browser's localhost does not reach the Mac's callback listener. Unsupported callback arrangements require deliberate forwarding or native Pi `/login`; Oqto does not relay arbitrary URLs.

Authenticated headless clients use:

```text
POST /api/runner-targets/{target}/provider-login
```

Commands: `providers`, `start`, `status`, `answer`, `cancel`, using the [private worker protocol shapes](../../tools/provider-login/README.md). Acting Account and credential paths cannot be supplied. `commit` is forbidden at this API: an authorized `status` request rechecks the grant and privately approves a pending save. Responses are `Cache-Control: no-store`; raw provider errors and credentials are not returned. Never log request bodies or verification challenges.

Closing the dialog cancels best-effort. Offline status cannot approve a credential save. The worker expires attempts after ten minutes; the runner additionally kills an unresponsive worker after the hard deadline/cancellation grace. Nonces and process generations fence stale approval and watchdog actions. Rejected stale requests do not kill another live attempt merely because their input is invalid.

`configured` means a stored credential entry exists, not that an upstream model turn succeeded. `saved_refresh_required` means Pi stored the credential but could not synchronize its local model state: refresh/reopen Pi rather than blindly repeating login. Credentials already committed cannot be rolled back by a late cancellation.

## Verification and delivery gates

- `PI_LOGIN_TEST_SDK=/absolute/pi/dist/index.js npm --prefix tools/provider-login run test:native`: native SDK persistence and lifecycle/security tests, using disposable profiles only.
- `cargo test --locked -j 1 -p oqto -p oqto-runner --lib`: includes explicit grant/Account denial, private Debug redaction, strict request fields and watchdog generation/expiry tests.
- Frontend package's Vitest runner: `tests/machine-providers.test.tsx`, `tests/sidebar-machines.test.tsx`, `tests/oqto-runner-targets.test.tsx`.
- Format/Clippy, TypeScript and useEffect/OqtoUI guardrails; regenerate bindings after Rust protocol changes.
- Live: prove mutual-TLS identity rejection, unauthenticated API rejection, browser commit rejection, owning Mac catalog and supported provider interaction, cancellation and unchanged credentials without human consent. Complete actual OAuth credential issuance only with the user's explicit provider consent.

A native fixture login is not a successful real OAuth/model turn. Record these separately in issue evidence.

Development proof (2026-09-08): authenticated original sidebar → Mac → Providers → OpenAI Codex → device flow produced a real verification code and an `auth.openai.com` link. The probe cancelled without opening the link or disclosing the code; the code/private prompt disappeared and the Mac credential contents remained unchanged. Final provider consent is the user's action. Native credential persistence was separately proved with disposable fixtures on Linux and Mac.

Current incremental-delivery debt: the clean parent worktree and implementation both report the same 41 Rust guardrail findings (including 30 in an external test module), and the existing canonical chat renderer has two unapproved effects. These are tracked separately; do not relax their baselines or misreport whole-repository gates as clean. Focused auth tests, Clippy and the OqtoUI type/architecture gates pass. Whole-frontend type checking also retains unrelated migration errors; no auth-file diagnostic was introduced.
