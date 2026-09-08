# Managed Pi provider login worker

Implementation checkpoint for `oqto-7e9y`. **This is a tested worker, not yet the Oqto browser login feature.** Runner wire operations, Account-authorized API routes, deployment packaging and frontend presentation remain to be integrated. Do not advertise provider login from inventory merely because this directory exists.

## Ownership and security checklist

- The runner supervisor selects the Pi SDK, private agent directory, machine and Account. None can come from browser request JSON.
- Use the **same managed Pi installation** as the owning runner. The worker requires the public `ModelRuntime` provider-auth API. Older installations fail closed; native Pi `/login` remains the fallback. Do not invent a cross-version auth-file writer.
- Pi handles OAuth, PKCE/callback validation, refresh and serialized credential persistence. Oqto does not copy credentials or rewrite `auth.json`. No chat Session or JSONL file is created.
- Native and browser presentations use the same provider interaction semantics. Browser URLs/device codes/manual-code prompts are private auth UI data, never chat messages, history-cache entries or telemetry.
- Literal API-key entry cannot introduce Pi `!command` or `$ENV` references. Those require separately authorized provisioning, not provider login privileges.
- Every control-plane operation must authorize the Account against the owning placement and explicit provider-login grant **before contacting the runner**. Inventory access is not a login grant. Never use a personal-runner fallback.
- The remote worker pauses before credential persistence. A control-plane supervisor must reauthorize the current Account/grant and issue the one-use `commit` nonce. **Never forward commit automatically without that authorization.** Credentials remain in the worker while this gate is pending.
- Cancellation/expiry before commit discards the result. A credential already committed by Pi cannot be rolled back by late cancellation. `saved_refresh_required` distinguishes a successful credential mutation from failed local model synchronization; do not blindly retry login.
- Only one login can run per worker, across all providers. Production integration must also prevent duplicate workers for the same credential store; a frontend process is not a credential-store lock owner.
- Fixed ten-minute attempt deadline, bounded frames/queue/events, request/prompt IDs and owner checks fence stale inputs. The supervisor must kill a worker that fails to settle after cancellation/expiry, and close it when its owner or placement is revoked.
- HTTPS authentication links are presented, not fetched or automatically opened. A localhost OAuth callback on the target is not reachable via the user's browser localhost. Use the provider's device flow or supported paste-back redirect/code; otherwise require deliberate callback forwarding or native `/login`.
- Custom extension discovery is intentionally not loaded into this privileged worker. Unsupported extension-owned login methods must remain unavailable rather than executing project extensions.

## Private headless protocol

Run under the owning runner principal, using its managed agent directory:

```sh
node tools/provider-login/worker.mjs \
  --sdk /absolute/path/to/pi/dist/index.js \
  --agent-dir /absolute/managed/home/.pi/agent \
  --machine mac --account alice
```

Private stdin/stdout only; no HTTP listener. Parent transport is responsible for Account authentication. Do not expose these pipes to agents or other Accounts. Errors are sanitized and credential values are never returned. Pi/provider console diagnostics are suppressed in the child.

One JSON object per **LF** record, maximum 16 KiB buffered input and 32 queued requests. The `id` is a bounded correlation identifier. Examples:

```json
{"id":"1","command":"providers"}
{"id":"2","command":"start","provider":"openai-codex","method":"oauth"}
{"id":"3","command":"status","attempt":"ATTEMPT_UUID"}
{"id":"4","command":"answer","attempt":"ATTEMPT_UUID","prompt":"PROMPT_UUID","value":"PRIVATE_INPUT"}
{"id":"5","command":"commit","attempt":"ATTEMPT_UUID","nonce":"COMMIT_NONCE"}
{"id":"6","command":"cancel","attempt":"ATTEMPT_UUID"}
```

A supervisor polls `status`, presents events and prompts, and sends `commit` only after fresh authorization when state is `awaiting_commit`. A native headless operator may explicitly approve that same step. Never put real secrets into shell command arguments or save input transcripts.

Result envelope: `{id, ok: true, data}` or `{id, ok: false, error}`. Terminal states are `saved`, `saved_refresh_required`, `failed`, `cancelled`, `expired`. Challenges are removed at completion. `configured` in the provider catalog means a stored entry exists, **not** that an upstream model request has succeeded. Ambient credentials are not inspected or exported.

## Verification

```sh
PI_LOGIN_TEST_SDK=/absolute/path/to/pi/dist/index.js \
  npm --prefix tools/provider-login run test:native
```

Without `PI_LOGIN_TEST_SDK`, the native integration test explicitly skips; that is not sufficient release proof. With it, tests cover actor/attempt ownership, concurrent starts, cancellation, expiry, stale prompt rejection, commit permits, revocation before commit, unsafe links, redacted failures, committed-but-unsynchronized state, request-field injection, and actual Pi persistence into a disposable directory. The native test proves unrelated credential entries survive and credential command/environment references cannot be introduced through login.

No real OAuth login or human consent has been performed by these tests. Live browser/device authorization remains an integration gate.
