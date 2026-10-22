# Isolated-agent brain grants and execution cwd

## Status

Proposed, with a tested in-process authorization primitive and sandbox cwd support implemented locally. Not integrated with live brain tools or an isolated-agent launcher. No runtime deployment, macOS execution proof, or Docker qualification.

## Context

An agent can have confined hands while retaining excessive access through brain-side history or memory tools. A shared brain process is not an OS isolation boundary between its conversations. Upstream Durable provides conversation-specific enabled tools, model-offered tools, and tool hooks; model visibility alone does not constrain nested calls or resource access. Recovery can execute a replay-safe tool without running its admission hook again.

## Decision

Keep upstream tool selection and use one semantic tool per domain with operation-specific resource authorization in the owning service. History and memory identifiers are opaque resource identities, not paths. Search must apply authorized scope before ranking, counts, and snippets; reads independently reauthorize. No global/local tool duplication or model-supplied caller identity.

A trusted supervisor owns grant mutation. Tools receive an object-identity-bound client for an exact Session/conversation. Child effective access intersects its requested grants with recursively delegable parent grants. No wildcard scope is currently supported. An omitted scope means the finite permitted resource set; explicitly unauthorized targets fail closed.

Grant updates publish new authority synchronously. Old invocations receive cancellation and their returned result is fenced, even after revoke/regrant. Update acknowledgment waits for those invocations to settle; an adapter ignoring cancellation can delay acknowledgment indefinitely. This primitive does not stream results: streaming adapters require per-disclosure enforcement. Previously disclosed context is not erased.

Keep TOML as the CLI policy input and the typed Rust library as the programmatic interface. Cwd is operational configuration, not a grant. `build_sandbox_command_with_cwd` and CLI `--cwd` resolve an existing directory within the authorized workspace, rejecting canonical escapes. They do not add mounts or make outside paths writable. Outside-workspace cwd is intentionally unsupported in this initial interface. Workspace-local policy is still loaded from the workspace, not the cwd.

## Implementation and boundaries

- `tools/oqt/brain-grants.ts`: closure-held controller and bound clients, finite operation/resource grants, live narrowing, revocation/result fencing, and explicit retirement.
- The controller is in-memory, not a durable policy store, authenticated operator API, or remote capability token. Restarts must reconstruct grants from trusted authority before binding clients; otherwise default deny. It is not registered with `openCodingHarness`.
- No history or memory provider adapter has been added. The present oqt harness registers coding tools and optional `request_access`; classic extension tools are not automatically native Durable tools.
- `backend/crates/oqto-sandbox/src/command.rs` and `cli.rs`: separate cwd selection; original command builder retains its workspace cwd default.
- Strong isolation from hostile code requires separate brain processes. These grants govern trusted dispatch, not arbitrary code in the same JavaScript process.
- Isolated-agent orchestration, hands-spec cwd propagation, Docker fallback, reviewed history transformation/import and upstream-update qualification remain outstanding.

## Verification

Eight Node authorization tests cover scope narrowing, operation isolation, immutable identity/grants, recursive delegation, live changes, draining ACKs, revoke/regrant fencing and retirement. Rust command tests cover cwd resolution, symlink escape denial, and identical compiled arguments except `--chdir`. A locally built Linux CLI ran `/bin/pwd` in a disposable nested cwd and rejected `--cwd ..`. These tests do not establish store-side search filtering or macOS/Docker isolation.
