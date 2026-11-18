# byteowlz-tui-kit (vendored)

- Source: https://github.com/byteowlz/templates, `rust-workspace/crates/byteowlz-tui-kit`
- Commit: e2fad8464f657692ef9394d00ae9cba23382a1a4 (2026-06-28)
- License: MIT (templates repository)
- Why vendored: the house TUI kit (tui-design skill) is not published to a registry and
  oqto builds offline and must not depend on a sibling checkout. Used by `oqt-tui`.
- Changes: `src/` is byte-identical to the source commit. `Cargo.toml` replaces the
  template workspace's inherited fields with explicit values and drops `[lints] workspace`
  (the kit is linted in its own repository, not under oqto's workspace lints).
- Refresh: copy `src/` from a newer templates commit, update this file, run
  `cargo test -p oqt-tui`.
