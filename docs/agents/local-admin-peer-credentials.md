# Local admin peer credentials

The existing admin Unix socket accepts root or the backend's OS uid; it remains
owner-only (0600). This is local OS administrative authority, not human MFA,
Workspace membership, or a fine-grained remote Operator capability.

The accept loop uses Tokio's platform `peer_cred` primitive. Missing/unsupported
peer evidence, syscall errors, and a different uid reject the connection before
serving HTTP. There is no fallback to the server's own uid. Linux keeps the same
root/backend-uid policy; macOS uses actual peer evidence rather than pretending
all peers are the server.

Proof:

```sh
cd backend
cargo test -p oqto --lib auth::local_admin
cargo check -p oqto
```

The tests inject unavailable/error/different-uid evidence into the actual policy
function and run a real same-uid Unix socket pair probe on the current platform.
They do **not** prove a live different-uid/root connection or Linux execution
when run on macOS. Those platform probes remain unverified; no root, host
permissions or services are changed by this patch.
