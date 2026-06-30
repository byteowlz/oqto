# byt is distributed as a private GHCR container image bundling its build toolchain

Status: proposed (2026-06-25). Implements the enforcement half of ADR-0021 (`byt release`); prerequisite for the reusable release workflow.

`byt release` (ADR-0021) is the org-wide release tool every byteowlz repo's CI invokes. Two facts shape how it reaches a runner: (1) `byt` is currently **private with zero releases** — not obtainable from a runner today; (2) `byt release` doesn't only need *itself* on the runner, it needs a **cross-compile toolchain** (`cargo-zigbuild`/`zig` for Rust, `go` for Go) to build the target app. "Get byt onto the runner" and "get the toolchain onto the runner" are therefore one problem.

## Decision

Distribute `byt` as a **prebuilt container image `ghcr.io/byteowlz/byt`** bundling `byt` + `cargo-zigbuild`/`zig` + `rust` + `go`. The reusable release workflow runs its job *inside* this image (`container: ghcr.io/byteowlz/byt:vN`) and calls `byt release`.

- **byt stays private.** A GHCR image in the byteowlz org is pulled from same-org Actions with the default `GITHUB_TOKEN` — no need to make `byt` public. (Intentional asymmetry: the *tools* — mmry/oqto/sx/… — are public release artifacts; the *release tool* is private.)
- **One pinned artifact** solves both byt distribution and the build toolchain; bump `:vN` deliberately.
- **Bootstrap once:** the first image is built from source (a one-off `byt`-repo workflow or local `docker build`). Thereafter byt's own releases self-host (run `byt release` inside the previous image), and every other repo simply `container:`s the image with no install step.

## Considered alternatives

- **`curl | sh` a prebuilt byt binary** — still requires installing rust/zig/go separately on every run; the container bundles both.
- **`cargo install --git` (with token)** — zero infra, but compiles byt on every run (slow). Acceptable as an interim to prove the flow before the image exists.
- **Make byt public + plain curl** — simplest consumption, but byt is internal tooling we keep private.

## Consequences

- `container:` jobs run **only on Linux runners** — which fully covers the ADR-0021 **required** targets (`x86_64`/`aarch64` linux, cross-compiled via `cargo-zigbuild`). 
- **Native macOS/Windows** builds (optional, where supported) run on `macos`/`windows` runners that can't use the Linux container; those legs need a prebuilt **byt binary** via a small `byteowlz/setup-byt` action — deferred until native darwin/windows is actually wanted.
- The byt-distribution + first-image bootstrap is the unlock that must land before `byt release` and the reusable workflow are usable (tracked as the first sub-steps of `oqto-4vjy.3`).
