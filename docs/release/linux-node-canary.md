# Frozen Linux Node candidate (oqto-bp7p)

Source scope: isolated `feat/linux-node-canary`; development candidate0.5.7, exact official Pi1.0.3 / Node24.14.1, coherent extension collection2.9.0 at `e70d7259cb1126eb8ddb4bea59236a390dedad32`, templates `277fbdc903e21c109e1c9d89b03610e8f33fc756`. No product-main merge, tag, publication, live install/restart or user-config change is authorized by this build.

## Composition and contracts

- Targeted runner ownership/socket/recovery guards ported from83379367, excluding unrelated tracker ledgers and preserving the release-owned Pi resolver.
- Full Linux distribution runtime mode is `bundled-official-node-npm`. The default stager obtains the official checksum-pinned Node archive and exact locked official npm closure, disables lifecycle scripts, and stages the adjacent-Node launcher. Explicit standalone acquisition remains a separate legacy operational path, not this bundle's runtime.
- `oqto-provisioning::bundled_runtime::validate` is the single read-only runtime declaration/file contract used by activation and doctor. It binds manifest version/runtime mode, Node archive hash, SDK integrity, lock hash and installed SDK version; missing/mismatched/unsupported tuples fail before activation. This is not independent publisher authentication or execution proof.
- `oqto-setup check-release` reuses full staged validation without activation/user writes; packaging invokes it before archiving.
- Canonical extraction continues to reject symlinks. Official Node/npm CLI aliases are materialized as bounded contained-target adjacent-Node launchers, preserving original JS module resolution. No npm/SDK pruning and no unsafe extraction relaxation. Alias tests reject outside/broken/directory/unknown-location links before any transformation.
- Doctor audits current process-private identity plus bundled runtime, and team users' home/history/shard/DB/WAL/SHM against resolved OS UIDs by metadata only. It never opens those SQLite files or repairs ownership. Current-profile/full-service readiness is separate from these checks.
- Canonical14-extension shipped set stays unchanged; experimental Durable workflow and runner-session-tools features are not silently added to release scope. Full coherent source is frozen; the shipped subset must pass real startup/RPC/control proofs.

## Qualification so far (before final artifact)

Runner/setup/provisioning tests pass; all-target Clippy-D warnings passes on runner/setup/provisioning/oqtoctl. Additional Node alias tests pass. Frozen extension source typechecks against the **exact shipped SDK namespace/closure**: an initial no-save SDK install pulled newer1.0.4 transitives, so qualification was corrected to bind all `@earendil-works` packages to shipped1.0.3 modules. Source package/lock files and owning repository are unchanged. Shipped14 plus combined native Pi1.0.3 catalog passed.

## Packaged doctor defect and bounded fix (oqto-vemr.5)

Initial6dc1d5e6 archive `d297837334e24fa319b3d1d5401fe69d904faf9fbc5050395619ae721b554dce` passes Ubuntu22 packaged startup and Ubuntu24 development0.5.6→0.5.7 activation/rollback/config preservation. It is retained as **superseded**, not silently replaced: actual strict JSON doctor returned0/findings=[] on an unprovisioned readonly container because `enabled=None, active=None` was ignored. Runtime corruption and nonregular WAL gates do reject and preserve metadata.

The bounded follow-up makes missing and unknown required service state an Error in the shared evaluator; missing/unknown/false/true regression states and actual-package doctor/activation/WAL checks become mechanisms. It does not claim complete oqto-vemr.5 coverage: unresolved personal template paths and declared socket/model checks still need actual fact collection/authorization/readiness design. No fake services/providers are supplied to make strict doctor green.

## Explicit release limits

Artifact installation/update/failed-activation rollback/config preservation and packaged runner/doctor traces remain to be recorded after the frozen build. Synthetic/readonly/no-sandbox fixture proofs are not full-shell, actual service/sandbox, real-provider, physical CPU/helpers or production authentication acceptance. Development0.5.6 predecessor is not published-history migration evidence. Current frontend is the previously integrated baseline; new Studio V4 production parity and runner-backed forks/subagents are not claimed.

The combined baseline retains inherited broad guardrail/front-end debt; source-local clean Clippy/tests do not waive that for publication. This is a disposable canary candidate, not a supported public release. Default CI acquisition of the frozen extension source also needs publication/availability verification; local clean frozen checkout is not proof of remotely fetchable source. Complete legacy setup/acquisition consumers must be audited before claiming easy-deployment end-to-end Node support.
