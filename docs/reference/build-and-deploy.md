# Build and deploy from published source

The public repository must contain the product source, locked dependencies,
synthetic tests, runtime schemas, licences and generic installation tooling.
Development notes, machine inventories, private captures and credentials are not
build inputs. Required defaults are reconstructed from the public repositories
and immutable commits declared in `dependencies.toml`; do not copy a developer's
agent configuration into a release. The reviewed `src/templates/embedded/` seeds
are product data: their source files are explicitly allowed in publication policy.
The agent seed is named `agent-instructions.md` in source and is materialized as
`AGENTS.md` only in a user's runtime profile. Repository-scoped agent instructions
remain private. Generated `dist/immutable/defaults/` staging is ignored by Git
and rebuilt from those pins, not tracked as a second source of defaults. Staging
must not make an otherwise clean public checkout appear to have source edits.

## Source builds

Use a clean checkout of one public commit. A normal build must not require private
Git access, a neighbouring checkout, or pre-existing `node_modules`/build output.
Frontend vendored archives must match their recorded provenance and lockfile.

The canonical packaging sequence is:

```bash
just dist-sync
just dist-stage-pi-runtime
just dist-stage-binaries --build
just dist-stage-frontend --build
just lint-dist-manifest-strict
just dist-package
```

Source staging defaults to public dependency URLs. A development build can set
`PI_AGENT_EXTENSIONS_URL` explicitly to use another source; public release pins
must remain available from the public mirror.

See `.github/workflows/release.yml` for the build-host prerequisites and supported
targets. Build prerequisites belong on the build host, not an installation host.
Run the relevant tests and validators before treating an artifact as deployable.

## Separate EAVS listeners

For a gateway with separate control and inference listeners, configure both URLs:

```toml
[eavs]
base_url = "http://127.0.0.1:3044"      # backend operator/control calls
inference_url = "http://127.0.0.1:3033" # ordinary scoped-key inference
# container_url = "http://host.containers.internal:3033"
```

Omitting `inference_url` retains existing unified-listener behavior. Local Session
environments, user model definitions and inference URL responses use the inference
URL. Containers still require an explicitly reachable `container_url`; a host's
loopback URL is not silently substituted. All configured endpoints must be absolute
HTTP(S) URLs without embedded credentials, query strings or fragments. Native
pi-eavs owns its separately consented server/key configuration in Pi.

The backend authenticates control requests (including health/provider metadata and
OAuth management) with the master credential and refuses redirects. If EAVS has
a distinct operator listener capability, set `listener_token = "env:EAVS_LISTENER_TOKEN"`
(or a private `file:` reference); it is sent only as `X-Eavs-Token` on control
requests. Omission supports EAVS's master-key listener fallback. Never add either
credential to user model definitions or weaken either gateway listener's
authorization. Before switching ports, privately back up gateway/backend config,
credential stores and databases; merge only declared changes. Verify control and
new-user/model resync as well as existing scoped-key catalog, streamed inference
and cold reopen. Keep a compatible binary/config/database rollback together.

## Installation and promotion

Deploy the exact checksummed artifact built from the selected public commit;
do not rebuild independently on every target. Keep dependency pins, source
revision, build target, artifact checksum and deployment receipts together.
Personal and shared installations can use the same artifact with separate private
runtime configuration. Promotion to either host is an explicit operator action,
not an automatic consequence of a new commit.

```bash
just deploy --config /path/to/private/hosts.toml --host personal-host \
  --artifact /path/to/oqto-VERSION-TARGET.tar.gz \
  --checksum /path/to/independently-obtained.sha256
```

The checksum must come from a trusted independent release source. Integrity alone
is not publisher authenticity. Activation, health checks and rollback stay in the
canonical installer; see [deployment](../../deploy/DEPLOY.md).

For an explicitly authorized source development deployment, use `--dev
--no-extension-update` on the public snapshot. Public dependency pins are immutable:
prepare changes in the working repository and publish a new checked snapshot.
Source builds use a private checkout fixed at one commit, with isolated build
outputs. Dirty source is refused, including dev mode.

## Configuration and rollback

Start with `deploy/hosts.toml.example` and supply an external private `--config`.
Actual host inventories, credentials, identity-provider settings, local paths and
user data are not part of the public source or release payload. Preserve existing
configuration; compare and merge defaults rather than overwriting it.

Rollback selects the previous verified artifact through the same installer.
A source checkout reset or a manual edit of an installed release is not rollback.

An isolated Vite/HMR preview can run from development source for rapid UI work.
It is not an installation qualification or permission to mutate an active host.
