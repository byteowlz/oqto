# Standard release packaging for byteowlz apps

Status: accepted (grilled 2026-06-25; rolled out 2026-07-02). Driven by ADR-0018 (one acquisition path); consumed by `oqto-setup acquire`. This is an **org-level** decision — oqto is the driving consumer and records it here; enforcement lives in the byteowlz org, not this repo.

`oqto-setup acquire` must fetch + checksum-verify prebuilt artifacts for every byteowlz tool. Today each repo packages differently, so the consumer carries a special case per divergence and silent drift goes unnoticed until a fail-closed download breaks. Observed 2026-06-25 against live releases:

- **Three artifact-naming schemes** the resolver must encode: Rust `{name}-v{ver}-{triple}.tar.gz`, Go (goreleaser) `{name}_{OS}_{arch}.tar.gz`, and oqto's bespoke bundle.
- **Inconsistent checksums:** 10/11 tools publish `checksums.txt`; **skdlr publishes none** — `acquire` fail-closed on it.
- **Divergent tarball layouts:** Rust tools are flat multi-binary; Go tools include `LICENSE`/`README`; the oqto bundle is a structured dir. The extract→PATH step can't be uniform.
- **Uneven target matrices:** mmry/sx ship darwin+windows; most ship linux-only.

We standardize release packaging across all byteowlz apps so the consumer collapses to one rule and drift becomes impossible.

## The spec

| Axis | Standard |
|---|---|
| Artifact name | `{name}-v{version}-{target-triple}.tar.gz` for **all** apps (Rust + Go); Go tools emit Rust-style target triples, not `{OS}_{arch}` |
| Checksums | `checksums.txt` per release (`<sha256>  <filename>` lines), **mandatory**; signature (minisign/cosign) optional, recommended |
| Tarball layout | executables under `bin/`; `LICENSE`/`README` at root |
| Targets | required: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`; add `*-apple-darwin`, `x86_64-pc-windows-msvc` where the app supports them (ADR-0020) |
| **glibc floor** | linux-gnu artifacts built against a **glibc floor of 2.28** via `cargo-zigbuild --target <triple>.2.28`, so prebuilt binaries run on older targets (RHEL8 / Ubuntu 18.04+). Without this, `ubuntu-latest`-built binaries require glibc ≥2.39 and **fail on Ubuntu 22.04** (demonstrated 2026-06-30). Dynamic-link only (NOT static/musl) — the host's patched glibc is used at runtime, so this is **not** a security regression. Artifact name keeps the plain triple (no `.2.28` suffix). |
| Tag | `v{semver}` |

## Where it lives / enforcement

The spec is implemented **once, as code, in `byt release`** — the existing byteowlz cross-repo meta-tool (same home as `byt design-system sync`) — so naming/checksums/layout/targets come from one tested implementation, not N hand-rolled workflows. Each repo's release is a thin **reusable GitHub Actions workflow at `byteowlz/byt/.github/workflows/release.yml` (byt is public, so any repo can call it directly — no separate `.github` repo needed)** that installs `byt` and runs `byt release`; consumer repos call it pinned to a tag, making each repo's own `release.yml` ~3 lines.

This gives: single source of truth (`byt`) + GitHub-native org-wide reuse (`.github`) + minimal per-repo surface. `byt release` may wrap cargo-dist/goreleaser internally, but **normalizes their output to this spec** — those tools are rejected as the public contract precisely because they produce provider-shaped, Rust-vs-Go-divergent output, which is the divergence being removed.

## Consequences

- `oqto-setup`'s `deps.rs` collapses from three `ArtifactNaming` schemes + the Oqto special case to **one** rule; the extract→PATH placement becomes uniform ("install `bin/*`").
- Multi-target acquisition (ADR-0020) becomes free.
- Drift like skdlr's missing `checksums.txt` becomes impossible — produced by construction.
- Migration cost: every byteowlz repo adopts the reusable workflow once (one-time per repo). Tracked org-side; the oqto-side cleanup (deps.rs simplification) is gated on repos conforming.
- **Near-term unblock (independent of the rollout):** skdlr must publish a `checksums.txt` now so the full managed `acquire` is green.
- **Cross-compilation rule for Rust apps with C deps:** apps pulling OpenSSL (via `git2`, native-TLS `reqwest`, etc.) must **vendor the C dependency** (e.g. `git2`'s `vendored-openssl` feature) — `zig cc` can't see Debian's multiarch system headers, and vendoring also drops the runtime libssl dependency. Verified on byt (its `git2`→`openssl-sys` build failed until vendored). Each migrating repo (eavs/mmry/trx/…) applies this in its own `Cargo.toml`.

## Rollout (2026-07-02)

Foundation proven on trx v0.6.3 + byt v0.5.2 (see wiki `projects/byteowlz-release-pipeline`). This session migrated the rest of the fleet and collapsed the oqto consumer.

**Repos migrated to the reusable workflow** (each got a `byt.release.toml` + a 3-line caller `release.yml`): eavs, mmry, agntz, tmpltr, sldr, skdlr, ignr (Rust) and sx, scrpr (Go). `trx` was the pilot. All 9 configs validated with `byt release targets`.

**oqto is intentionally NOT on the byt workflow.** The platform bundle is a subdir workspace (`backend/`) that also packages the built frontend + `oqto-browserd` (bun) and links `libseccomp` (linux-only); byt builds single-language `bin/`-only tarballs and can't bundle those. oqto keeps its bespoke `release.yml`, which **already conforms to this spec** for the axes `acquire` consumes: `oqto-v{ver}-{triple}.tar.gz` naming, `bin/` layout, and a combined `checksums.txt`. That is sufficient — the consumer treats oqto like any other artifact.

**byt gained two AUR-correctness features** this rollout exposed:
- `[release.aur].depends` / `.optdepends` — emitted into PKGBUILD + `.SRCINFO`. Needed so `tmpltr` keeps its `typst` runtime dependency (the generator previously had no way to declare deps).
- `provides`/`conflicts` now **omit the field when empty** instead of defaulting to `(name)`. A bare bin package (pkgname == binary, e.g. `eavs`) declares neither; forcing `conflicts=('eavs')` on package `eavs` is a self-reference namcap flags. `-bin` packages still set them explicitly.

**Deliberately dropped** (flag if still wanted; not reproduced by the uniform pipeline):
- `mmry-cuda` — the CUDA-linked AUR variant. The uniform pipeline ships one prebuilt package.
- `sx-search` **source** AUR package — only the prebuilt `sx-search-bin` is published now.
- skdlr **Scoop** (Windows) manifests — byt has no Scoop backend yet. (A future byt enhancement, mirroring the Homebrew dispatch.)
- `scrpr` AUR converts from source-built to a prebuilt-binary package under the same name.

**Consumer collapse (`oqto-setup/src/deps.rs`)** — done. The `ArtifactNaming` enum (`RustTarget`/`GoReleaser`/`Oqto`) + `classify()` + the `go_os`/`go_arch` tokens are deleted; `artifact_url` is one formula for every tool; `Arch::target()` returns the plain triple. `install_staged` (in `acquire.rs`) was already layout-aware (shallowest `bin/`, flat fallback), so extract→PATH was uniform already. This closes oqto-4vjy.5; skdlr's missing `checksums.txt` (oqto-4vjy.2) is satisfied by construction once skdlr releases via the shared pipeline.

**Per-repo release execution** (bump version, tag `vX.Y.Z`, push) remains the maintainer's cadence — the migration only stages the *mechanism*. byt v0.5.3 (the depends/optdepends + omit-empty change) must ship first so the `:latest` builder image carries it before the AUR-affected repos (tmpltr especially) cut a release.
