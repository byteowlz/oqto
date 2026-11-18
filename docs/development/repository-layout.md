# Repository layout and source hygiene

The repository is product source, not a runtime store or a scratch archive.
`just test-repository-hygiene` checks tracked/indexed paths against the root
allowlist in `scripts/lint/repository_hygiene.py`. New root entries require an
explicit change to that contract. Ignored local files are not scanned or deleted.

## Boundaries

| Location | Responsibility |
| --- | --- |
| `backend/` | Rust product crates, crate-owned tests and migrations |
| `frontend/` | Web client, client tests, and checksum-qualified vendored inputs |
| `contracts/`, `fixtures/` | Versioned semantic contracts and synthetic test data |
| `dist/manifest.toml`, `dist/mutable-templates/` | Release declarations and default configuration sources |
| `scripts/` | Build, setup, deploy, development, lint and qualification tooling |
| `tools/` | Explicitly scoped companion components and experiments with their own README and gates |
| `docs/` | Current documentation and architectural decisions; `CONTEXT.md` defines domain terms |
| `diagrams/`, `logo/`, `banner.png` | Referenced architecture and product/brand source assets |
| Root manifests, guides and `setup.sh` | Repository-wide contracts and documented bootstrap entrypoints |

Private maintainer context is not product configuration. Existing `.pi/`,
`.oqto/`, `machines/` and agent instructions must not be copied into user homes
or deployed as defaults; public-source exclusions are defined separately in
`publish.toml`. Moving files never grants publication or deployment authority.

## Source and installed oqt

The experimental Linux source entrypoint is `tools/oqt/dev.sh`, exposed through:

```sh
just oqt --help
just oqt --workdir /path/to/project
just test-oqt-source
```

The project work directory must be outside this source tree: a coding agent
must not edit its own active approval machinery. Tests pass an installed
launcher explicitly when exercising a staged runtime; they do not infer one
from a source directory or silently fall back to it.

`just install-oqt` remains the explicit developer-only installation experiment.
The installed `oqt` command executes its independent content-addressed runtime;
relocating the source launcher does not relocate, replace or activate it. This
is not a second production installer (ADR-0066).

## Generated output and retained evidence

Never commit `.tmp/`, dependency directories, Python bytecode/cache directories,
Cargo targets, frontend build output, or generated `dist/immutable` staging.
The hygiene gate rejects these even when force-added through an ignore rule.
Release packaging uses the declared staging commands and manifests; verified
vendor archives and synthetic fixtures are source inputs, not arbitrary caches.

Keep raw execution evidence, transcripts, screenshots and build artifacts in
private local state or an explicitly approved artifact store, outside source.
Retain useful provenance with hashes/manifests. Review sensitivity and licensing
before moving evidence into any hosted system. Do not move private screenshots
or real conversations into a shared design gallery.

`artifacts/`, `design-exploration/` and `mockups/` currently retain historical
design sources. They are not an approved design contract or a default home for
new experiments. Preserve them until their references, provenance and synthetic
review suitability are checked; migrate useful design records through the shared
Design Studio, not another product-local gallery. Their existing presence in the
root allowlist is a retention boundary, not permission to proliferate them.

## Cleanup and verification

The change-check wrapper stamps successful quick/full profiles by source content.
It uses `sha256sum` or macOS `shasum -a 256` and refuses to certify a run when
neither tool is available.

1. Back up unpublished refs, every stash entry, and staged/unstaged/untracked
   source before destructive cleanup. Git bundles of `--all` alone do not preserve
   older stash reflog entries or uncommitted files. Keep private backups outside Git.
2. Check callers, runtime paths and documentation before moving an entrypoint.
   Update the callers together; do not leave dead compatibility wrappers.
3. Remove disposable tracked output without deleting unrelated ignored local data.
   Worktree removal additionally needs active-owner/build confirmation.
4. Run focused regressions, `just test-repository-hygiene`, and the existing gates
   owed by the changed component. A layout cleanup does not qualify experimental
   features or turn known failing application suites green.
