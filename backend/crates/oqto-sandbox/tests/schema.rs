//! Keeps `sandbox.schema.json` and the shipped configs honest.
//!
//! The schema is derived from `SandboxConfigFile`, so it cannot describe a
//! format the parser does not implement. Every config in the repo is then
//! parsed with that same parser, which rejects unknown keys.

use std::path::{Path, PathBuf};

use oqto_sandbox::SandboxConfigFile;

fn repo_root() -> PathBuf {
    // crates/oqto-sandbox -> crates -> backend -> repo
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

fn schema_path() -> PathBuf {
    repo_root().join("backend/crates/oqto/examples/sandbox.schema.json")
}

/// Every `sandbox.toml` the repo ships or deploys.
fn config_corpus() -> Vec<PathBuf> {
    let root = repo_root();
    let mut found = Vec::new();

    let mut stack = vec![
        root.join("backend/crates/oqto/examples"),
        root.join("deploy"),
        root.join("dist/immutable/defaults/workdir-templates"),
    ];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "node_modules") {
                    continue;
                }
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == "sandbox.toml" || n.starts_with("sandbox.template."))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn checked_in_schema_matches_the_types() {
    let generated = SandboxConfigFile::schema_json();
    let path = schema_path();
    let checked_in = std::fs::read_to_string(&path).unwrap_or_default();

    // Compared by value but reported by summary: dumping two multi-thousand
    // line schemas into the failure output helps nobody.
    if checked_in.trim() != generated.trim() {
        let checked: serde_json::Value =
            serde_json::from_str(&checked_in).unwrap_or(serde_json::Value::Null);
        let fresh: serde_json::Value = serde_json::from_str(&generated).expect("generated is json");
        let keys = |v: &serde_json::Value| -> Vec<String> {
            v.get("properties")
                .and_then(|p| p.as_object())
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default()
        };
        let old = keys(&checked);
        let new = keys(&fresh);
        let missing: Vec<_> = new.iter().filter(|k| !old.contains(k)).collect();
        let extra: Vec<_> = old.iter().filter(|k| !new.contains(k)).collect();
        panic!(
            "{} is stale; run `just schema-update`\n  missing from file: {missing:?}\n  no longer in types: {extra:?}",
            path.display()
        );
    }
}

#[test]
fn every_shipped_config_parses() {
    let corpus = config_corpus();
    assert!(!corpus.is_empty(), "corpus discovery found nothing");

    let mut failures = Vec::new();
    for path in &corpus {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                failures.push(format!("{}: unreadable: {e}", path.display()));
                continue;
            }
        };
        if let Err(e) = toml::from_str::<SandboxConfigFile>(&text) {
            failures.push(format!("{}: {e}", path.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "shipped sandbox configs do not parse:\n{}",
        failures.join("\n")
    );
}

/// A config is only useful if its keys survive into the effective policy, so
/// assert the shipped templates actually change something.
#[test]
fn shipped_templates_take_effect() {
    let root = repo_root();
    let template = root.join("backend/crates/oqto/examples/sandbox.template.strict-infra.toml");
    if !template.exists() {
        eprintln!("skipping: template not present");
        return;
    }

    let text = std::fs::read_to_string(&template).expect("readable");
    let file: SandboxConfigFile = toml::from_str(&text).expect("template parses");
    let config: oqto_sandbox::SandboxConfig = file.into();

    assert!(
        config.deny_read.iter().any(|p| p == "~/.kube"),
        "the template's deny_read must reach the effective config"
    );
}

/// Guards the discovery itself: if the walk silently stops finding configs,
/// `every_shipped_config_parses` would pass while checking nothing.
#[test]
fn corpus_discovery_finds_the_shipped_configs() {
    let corpus = config_corpus();
    assert!(
        corpus.len() >= 8,
        "expected the shipped configs and workdir templates, found {}: {corpus:#?}",
        corpus.len()
    );
}
