# ADR-0061: Account sign-in with passkeys and optional external identity providers

## Status

Proposed (2026-10-16). Account-facing counterpart of ADR-0029 (remote operator
authentication): both feed the same `Identity + AuthenticationAssurance` model
and must not fork authorization by authenticator. Tracked by `oqto-h9bb.2`
(WebAuthn) and `oqto-h9bb.5` (OIDC); organization SSO gate `oqto-v6hw`.

## Context

People sign in to Oqto (Web, Desktop, Mobile, `oqtoctl`) with a username and a
bcrypt password; the backend issues an HS256 JWT in an `auth_token` cookie or
bearer header. Three gaps:

- **No phishing-resistant factor.** Password-only, no MFA, no passkeys.
- **No external sign-in.** `auth.oidc_issuer`/`oidc_audience` only validate
  RS256 tokens minted elsewhere; Oqto cannot run a "Continue with ..." flow.
  Institutional installs (Fraunhofer: Microsoft Entra) need SSO.
- **No server-side sessions.** Logout is client-side; a stolen JWT stays valid
  until expiry, and a person cannot see or revoke their other sessions.

Reference point: Stencil's self-built identity service (`auth.stencil.so`)
offers password, passkeys, Google/GitHub, TOTP + recovery codes, session list
and revoke, and is itself a standard OIDC provider (PKCE, device flow, ES256)
used by all their apps. It shows the target UX; it is not a dependency.

## Decision

### 1. One Account, many credentials

An Oqto Account (CONTEXT: Account) owns zero or more **sign-in credentials**:

| Credential | Purpose |
|---|---|
| Password (bcrypt, existing) | Default; can be disabled per Account once a passkey exists |
| Passkey (WebAuthn) | Primary phishing-resistant sign-in and second factor |
| TOTP + one-time recovery codes | Second factor where passkeys are unavailable (not privileged-grade, ADR-0029 §4) |
| External identity link | `(issuer, subject)` from a configured provider |

Credentials authenticate; they never carry roles. Every successful sign-in
produces `Identity + AuthenticationAssurance` (methods/`amr`, assurance class,
auth time, issuer), and authorization only reads the internal assurance class.

### 2. Passkeys in-process

Implemented in the backend with `webauthn-rs`; credentials (credential ID,
public key, sign counter, transports, user handle, label, created/last-used)
live in Oqto's database. Relying-party ID and allowed origins come from
configuration and must match the served origin; a mismatch fails closed at
startup, not at the first sign-in. Supported flows: discoverable passkey sign-in
(no username), passkey as second factor after password, add/rename/remove in
Account settings, and re-authentication (step-up) for sensitive Account and
admin actions. Removing the last strong factor requires a recent step-up.

### 3. External providers are optional, configured, and off by default

```toml
[[auth.providers]]
id = "entra"                  # stable, used in callback URL and links
kind = "oidc"                 # oidc | github
display_name = "Fraunhofer"
issuer = "https://login.microsoftonline.com/<tenant>/v2.0"
client_id = "..."
client_secret = "kyz:oqto/entra-client"   # resolved like other secrets; never literal in docs
scopes = ["openid", "profile", "email"]
signup = "invite-only"        # invite-only | allowed-domains | open
allowed_domains = []
```

- `kind = "oidc"` uses the `openidconnect` crate: discovery, Authorization Code
  + PKCE (S256), `state` and `nonce`, issuer/audience/signature/expiry checks.
- `kind = "github"` uses the `oauth2` crate plus GitHub's user and verified
  email APIs, because GitHub is OAuth2 without OIDC; it yields password-equivalent
  assurance only.
- No `[[auth.providers]]` entry means no button and no route: installs behave
  exactly as today. Unknown `kind` or `signup` values fail at startup.
- Provider `acr`/`amr` are mapped to Oqto assurance only through explicit
  configuration (ADR-0029 §4); otherwise a provider sign-in counts as single
  factor.

### 4. Linking never matches by email alone

An external identity is bound to an Account by `(issuer, subject)`:

- **Signed in, then "Connect"**: the logged-in person links a provider account
  after a fresh provider sign-in.
- **First sign-in with an unlinked identity**: governed by the provider's
  `signup` policy. `invite-only` (default) requires a valid invite; matching an
  existing Account's email offers "sign in with your existing method to link",
  never automatic merge, even when the provider says the email is verified.
- Unlinking requires another remaining sign-in credential.

### 5. Server-side sessions

Sign-in creates a session record (ID, Account, credential used, assurance, created,
last seen, coarse client description). The cookie/bearer token references it and
stays short-lived; the session is checked on every request (cached briefly) and on
WebSocket attach. People can list and revoke sessions ("sign out other devices");
password change, credential removal and admin disable revoke sessions. Cookies are
`HttpOnly; Secure; SameSite=Lax` (Lax so provider callbacks return); state-changing
endpoints additionally require CSRF protection.

### 6. Oqto as an identity provider is out of scope here

Running Oqto itself as an OIDC provider (one login for Web/Desktop/Mobile/Apps,
device flow for `oqtoctl`) is a later, separate ADR, compared against embedding a
self-hosted IdP such as Zitadel or Authentik. Nothing in this ADR may preclude it:
the session and credential stores above are what such a provider would issue from.

## Delivery

1. Session store, revocation and session list (prerequisite for everything below).
2. Passkeys: register, sign in, second factor, step-up, settings UI.
3. TOTP and recovery codes.
4. Generic OIDC providers with linking and signup policy; Microsoft Entra and
   Google verified.
5. GitHub OAuth2 adapter.

Each step ships behind its configuration and is usable on Web plus a documented
headless/CLI path (`oqtoctl login` opens a browser or prints a verification URL;
it never asks to paste a token).

## Verification

- WebAuthn: wrong origin/RP ID, replayed challenge, expired challenge, cloned
  authenticator (counter regression) and unknown credential all fail closed.
- OIDC: wrong issuer/audience/signature, missing or replayed `state`/`nonce`,
  missing PKCE verifier and expired tokens fail; tests against a local mock IdP.
- Linking: an unlinked identity whose email matches an existing Account never
  signs into it; unlink of the last credential is refused.
- Sessions: revoked or expired sessions are rejected on HTTP and closed on
  WebSocket within the cache bound; password change revokes other sessions.
- Config: no providers configured = no new routes and unchanged login page;
  invalid provider configuration fails at startup with a precise error.
- Secrets (client secrets, codes, tokens) never appear in logs, URLs after the
  callback, audit fields or frontend state.

## Rejected alternatives

- **Hosted auth (Clerk, Auth0, WorkOS):** contradicts self-hosting and offline
  installs; introduces a third party into every sign-in.
- **Match external identities to Accounts by verified email:** a provider or
  domain the operator does not control could take over an Account.
- **Keep stateless JWTs only:** makes logout, revocation and session lists
  impossible.
- **Make an embedded IdP mandatory now:** large operational surface for personal
  installs that only need passkeys.

## Consequences

- Personal installs gain passkeys without any external dependency; institutions
  get SSO by configuration.
- Requests gain a session lookup; it is cached and bounded.
- Frontend login and Account settings grow (passkeys, providers, sessions); Mobile
  and Desktop must support passkey and browser-based provider sign-in.
