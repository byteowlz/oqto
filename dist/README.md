# dist/

Distribution staging source of truth for Oqto packaging.

- `manifest.toml`: authoritative shipped asset manifest.
- `immutable/`: versioned, release-owned assets (never edited in place on host).
- `mutable-templates/`: copy-once seed templates for admin/user editable files.

This tree is intended to be the only source for release payload composition.

Validation:

```bash
just lint-dist-manifest
just lint-dist-manifest-strict
```

Workflow:

```bash
just dist-sync
just dist-stage-binaries --build
just lint-dist-manifest-strict
just dist-package 0.0.0-dev local
```
