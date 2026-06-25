# One install/update/deploy engine: one activation path, one artifact-acquisition path

Status: proposed; partially implemented (consolidates epic `oqto-vemr`; supersedes the bash-first operational layer that grew around ADR-0016). Refined 2026-06-25 for portability (ADR-0020): the bundle is multi-target and activation is supervisor-agnostic.

ADR-0016 defined the right model — one host layout, one `dist/manifest.toml`, one transactional activation (release stage → validate → symlink switch → relink bins → doctor). The *implementation* never converged on it: ~11k lines of bash (`scripts/deploy.sh` 2178 LOC + 21 `scripts/setup/*.sh` modules + dist scripts) carry the operational logic, while the typed core (`oqto-provisioning` + `oqto-setup`, ~1.1k LOC) that proves the model is shadowed by bash reimplementations. We decide to collapse onto the typed core and delete the duplicates.

## The two duplications we are deleting

**1. Activation is implemented twice.** `oqto-setup install` already does artifact-install + checksum + transactional activation + bin relink + doctor hook (the ADR-0016 contract). `scripts/deploy.sh` *also* re-implements staging / activate / rollback / prune / health as ~1200 lines of bash, and that bash is what runs in practice. There must be exactly one activation engine: `oqto-setup install`. `deploy.sh` keeps only what is genuinely shell's job — resolve SSH targets, iterate hosts, push the artifact, invoke `oqto-setup install` on each — and loses every stateful/failure-prone step (staging, symlink switch, rollback, prune, health gate, migration orchestration), which move into the typed core.

**2. Binary acquisition is implemented four times.** "Get binary X onto a host" exists as four divergent code paths:
- `scripts/install.sh` — download release tarball + `oqto-setup install` (the good path)
- `deploy/docker/Dockerfile` downloader stage — downloads *every* binary (oqto's + all byteowlz tools) from GitHub releases, zero compilation (proof the whole stack assembles from prebuilt artifacts today)
- `scripts/setup/08-agent-tools.sh` `download_or_build_tool` — release download, fall back to `cargo install --git`
- `scripts/deploy.sh` `remediate_dependency()` — release download, fall back to cargo install from source

There must be exactly one acquisition path: **manifest-driven download of prebuilt, checksummed artifacts**, owned by the typed core and consumed by install, dev setup, and deploy alike. The Docker downloader stage is the reference behavior; the other three collapse into it.

## Decision

- **`oqto-setup` is the single install/update/deploy engine.** `install`/`update`/`deploy` are the same transactional activation underneath — `update` is "install a newer artifact + activate"; `deploy` is "do that on remote hosts over SSH." No second implementation of activation in any language.
- **One artifact-acquisition path**, driven by `dist/manifest.toml` (asset class/dest/method, ADR-0016) plus `dependencies.toml` (version pins). It downloads prebuilt, checksummed binaries for every component in the manifest — oqto's crates *and* the byteowlz deps (trx, mmry, eavs, agntz, …).
- **`release.yml` publishes a complete prebuilt bundle**, not just oqto's own binaries. The bundle is the unit of install/update/deploy. The byteowlz sibling tools are consumed as released artifacts pinned by `dependencies.toml`, never built from sibling source on a target or in the user install path.
- **The bundle is multi-target** (per target triple), and the acquisition resolver selects the artifact matching the host target (ADR-0020). Linux `x86_64`/`aarch64` are the near-term targets; cross-compiled macOS/Windows runner artifacts are reachable later without changing the resolver. `release.yml` currently ships only `x86_64` and must be widened.
- **Activation is supervisor-agnostic** (ADR-0020). The engine stages + atomically switches the release and relinks bins, then delegates the service restart/reload step to the **Placement Supervisor** — it does not hardcode `systemctl`. systemd is one backend; the engine must work unchanged when the supervisor is podman/k8s/local-process. The engine is also **placement-agnostic**: it serves the current host/ssh deployments today and the container image (ADR-0019) later from the same bundle.
- **Build-from-source is a dev-only path**, reachable only behind an explicit flag (`just install --from-source` / dev profile). It is never on the install, update, or deploy critical path. `just install-deps` stops building ~12 sibling repos by default.
- **Bash is a thin shim.** `scripts/install.sh` (curl-able bootstrap) and the remote-host iteration in deploy stay in shell; everything stateful, validated, or recoverable lives in Rust where it is typed, unit-tested, and shares the `oqto-provisioning` contract evaluator. Migration/recovery (oqto-log cutover, corruption heal) are explicit `oqto-log`/`oqto-setup` subcommands deploy *gates on*, never inline bash heuristics.

## Non-goals

- Not a redesign. ADR-0016's layout, manifest, file-class model, and activation contract stand unchanged — this ADR is about deleting the bash that shadows them.
- Not removing remote/SSH orchestration — only shrinking it to target-resolution + per-host `oqto-setup install`.
- Not changing what gets installed (component set), only *how* it is acquired and activated. Reducing the mandatory component surface (optional feature bundles for typst/slidev/whisper/voice) is related but tracked separately.

## Implementation status

On branch `feat/oqto-setup-engine-vemr` (commit `10401ec`):

- **Landed:** the transactional activation engine in `oqto-setup` (`src/main.rs`), the manifest-driven acquisition resolver (`src/deps.rs`) and download driver (`src/acquire.rs`). 22 tests green; clippy + fmt clean. Covers the engine side of `oqto-vemr.3` and the resolver/download side of `oqto-vemr.9`.
- **Pending (host-gated, do with oversight on a box that can exercise install/deploy):**
  1. Wire `just install-deps` → `oqto-setup acquire` (additive, reversible — start here).
  2. `release.yml`: publish the full multi-target bundle + per-artifact `.sha256`.
  3. Delete the four duplicate acquisition paths and `deploy.sh`'s activation bash (the destructive half — only after 1–2 prove out).

## Consequences

- `scripts/deploy.sh` shrinks from ~2178 lines to a thin SSH/iteration driver; the legacy prepare/activate/rollback/prune/health bash is deleted (`oqto-vemr.3`).
- The four acquisition implementations collapse to one; `08-agent-tools.sh` and `remediate_dependency()` are deleted in favour of the manifest-driven downloader (new child of `oqto-vemr`).
- `oqto-setup install` gains whatever activation responsibilities currently live only in `deploy.sh` (ordered restarts, health gate, prune, rollback) so it is a complete engine, with the SSH wrapper calling it.
- The 21 `scripts/setup/*.sh` modules become typed `oqto-setup hydrate` steps driven by the manifest (config-rendering modules like `10-eavs.sh`/`09-searxng.sh` move into Rust); follow-on to `oqto-vemr.4`.
- Doctor (`oqto-provisioning::evaluate_manifest`) is the single verifier shared by install, deploy preflight, and `oqto-setup` — no parallel gate logic in bash (`oqto-vemr.5`).
- Privileged steps (sudoers, unit/socket setup, container lifecycle) route through the `oqto-hostd` broker (ADR-0009), not scattered `sudo` calls in scripts (`oqto-vemr.6`).
- Enables the E2E matrix (`oqto-vemr.8`) to test one engine instead of N bash paths.
