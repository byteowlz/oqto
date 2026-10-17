# ADR-0062: Backendless personal host device pairing

Status: Accepted contract; implementation and native acceptance in progress (`oqto-8x3p.10.7`).

## Context

A personal owner must be able to run a command on a Mac/Linux host and scan a QR in an Oqto client to use sessions, Files and terminals directly. An Oqto backend, platform Account or centralized MFA deployment is not a prerequisite. ADR-0029/0061 govern the optional centralized Account path; their assurances must not be fabricated for a locally paired Device. Existing mTLS inventory/Files policy in ADR-0057 is a distinct transport, not an automatic pairing grant.

## Decision

The Runner owns personal host access and local authorization state. There is no second session/PTY supervisor. A short-lived, single-use QR ticket pins the HTTPS server identity and lets a client prove possession of an Ed25519 key. Persist a paired Device and only the independently selected, owner-approved capability grants from the ticket; a client cannot submit its own grants. Rooted Files read/write, session observe/control and full-Principal terminal access remain distinct. Full-Principal terminal or unmanaged agent control is broad execution authority, not a directory sandbox.

Reuse CrossLatch's store-independent `xlatch-wire-auth`, pinned to an immutable revision, without its job scheduler, image stack or SQLite dependency. Preserve CrossLatch's existing wire; Oqto uses different compiled signing domains. Signature verification is not authorization: the owning store atomically consumes tickets/nonces, checks current device/grant revision and revocation, and maintains a durable clock watermark for replay pruning. Device keys are distinct from Accounts and OS Principals. Local owner administration has the host user's trust boundary; it does not claim protection from arbitrary unsandboxed execution as that same user.

HTTPS identity pinning is mandatory. Never publish a raw Pi Unix socket. Every privileged route/frame checks the paired Device's current grants before resource access; bounded streams must also close on revocation. A device-authenticated connection does not approve a tool call, transfer a terminal input lease or establish confinement. Session control still requires native terminal confirmation and a single-writer lease. Disconnect/revoke/reclaim releases remote authority without terminating terminal-owned Pi work, and uncertain input is never automatically replayed.

Pi owns native JSONL; oqto-log owns canonical Chat/history. Existing-session adoption uses durable identity mapping and a no-second-Pi fence; native event sequence, text or visible order cannot become canonical replay identity. Files use descriptor-relative confinement, explicit granted roots, pinned root identity and stale-baseline write rejection. PTYs use Runner lifecycle, scoped ownership and explicit reconnect/input fencing; granting terminal access is visibly full-user authority.

The CLI, native clients, Webbench and headless clients share semantic contracts. A pairing QR is sensitive enrollment material, emitted only intentionally, not a log receipt. Preserve unknown user configuration, existing state, native profiles, sessions and credentials. Do not replace a running transport-only checkpoint until the required functional slice is qualified.

## Verification and remaining scope

Shared crypto tests are not service, Mac or phone acceptance. Require real pinned-TLS enrollment and direct client operation, single-use/expiry/nonce/domain denial, concurrent enroll/revoke tests, independent grant checks before filesystem/socket access, rooted file escape/stale-write tests, native PTY resize/reconnect/ownership tests, actual existing-Pi adoption/control/history/reload without duplicate writer, and preserve-first Mac/iOS deployment receipts. Implementation remains incomplete until these pass; this decision does not assert those results.
