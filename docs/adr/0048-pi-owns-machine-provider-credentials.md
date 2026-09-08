# Pi owns machine-scoped provider credentials

Accepted (2026-09-08), `oqto-7e9y`. Extends [runner inventory](0047-runner-inventory-is-not-session-placement.md), not Session placement or [App credential mediation](0046-app-egress-credential-mediation-and-mcp-adapters.md).

Oqto presents Pi's provider-auth interactions through an explicitly granted, single-Account runner. The owning runner uses its managed Pi SDK and native credential store; neither the backend nor browser becomes an OAuth implementation or credential replica. EAVS remains optional for single-user deployments. Provider/model definition synchronization is separate, entry-level work; OAuth credentials remain machine-local by default.

A private, sandboxed worker holds the provider result until the backend reauthorizes the current Account and grants a one-use save permit. The browser cannot submit that permit. This avoids retaining authority across a long device/browser flow merely because login was initially allowed. Late cancellation cannot undo a credential Pi has already committed. Attempts are ephemeral, bounded, and never Chat Sessions or history events.

The initial adapter deliberately rejects shared/multi-user runners, missing sandbox enforcement, and unsupported managed SDKs rather than inventing an auth-file compatibility writer. Transport certificates/control sockets remain sandbox-inaccessible. Native Pi credentials retain Pi's profile trust model: this is not a vault or a promise that agent code sharing a readable Pi profile cannot access its model credentials. General App secret mediation remains separate.

Configuration, supported interactions, security checks and verification: [provider login runbook](../development/provider-login.md).
