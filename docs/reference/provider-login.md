# Runner-scoped provider login

Provider login acts on one explicitly selected Runner credential profile. It
requires both an Account-authorized backend grant and the owning Runner's
provider-login configuration. SSH reachability, an administrator role or a
visible Machine does not grant credential access.

## Configuration

Preserve and back up existing configuration, then merge only the required fields.
Enable `provider_login = true` on the existing backend target with exactly one
owning Account in its Account allowlist. On that Runner:

```toml
[runner.provider_login]
account_id = "ACCOUNT_ID"
node = "/absolute/path/to/node"
worker = "/usr/local/share/oqto/provider-login/worker.mjs"
```

Use the owning Runner's supported managed Pi SDK and private agent directory.
The initial adapter supports a single owning credential profile, not shared
multi-user credentials. Unsupported SDKs and missing grants must fail closed.
Do not copy a personal `auth.json`, change another deployment's HOME, or start a
second Runner against the same store to enable login.

For remote Runners, verify mutual TLS and exact peer/Account identity. A private
network or SSH forward is transport, not an authorization grant. Keep listener
keys and control sockets outside workload-readable paths. Never expose an
unauthenticated Runner listener.

## Browser and headless operation

On supported surfaces, select the explicitly granted Machine and open Providers.
A remote browser cannot reach the Runner's localhost OAuth callback implicitly;
use a supported device/code flow or deliberate forwarding. Provider consent
remains the user's action.

Authenticated headless clients use `POST
/api/runner-targets/{target}/provider-login` with `providers`, `start`, `status`,
`answer` and `cancel`. The API does not accept credential paths or an acting
Account from request JSON, and browser clients cannot issue the worker's private
`commit` operation. Fresh authorization is required before a credential save.

Challenges, secrets and raw provider failures must not enter chat history,
request logs, URL parameters or persistent browser caches. Closing the dialog
cancels best-effort; expiry and process generations fence stale responses.
`configured` means a credential entry exists, not that a model request succeeded.
Revoking transport does not revoke an already issued provider token.

See the [worker contract](../../tools/provider-login/README.md) for private
parent/worker messages and disposable native integration tests. Those tests do
not constitute real OAuth consent or a successful upstream model turn.
