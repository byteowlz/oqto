//! Classifies owner-requested repairs without weakening ordinary pin validation.
use super::{NativeHeader, SourceIdentity, SourcePin};
use std::path::Path;

pub(super) enum Eligibility {
    SameSource,
    TrustedRewrite,
    Mismatched,
    Unrecognized,
}

pub(super) fn classify(
    header: &NativeHeader,
    stored: &str,
    workspace: Option<&Path>,
) -> Eligibility {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stored) else {
        return Eligibility::Unrecognized;
    };
    let trusted = workspace == Some(header.workspace.as_path());
    if value.get("pin").is_some() {
        let (Ok(mut old), Ok(now)) = (
            serde_json::from_value::<SourcePin>(value),
            serde_json::from_str::<SourcePin>(&header.identity),
        ) else {
            return Eligibility::Unrecognized;
        };
        if old.pin != 2 {
            return Eligibility::Unrecognized;
        }
        let rewritten = old.ino != now.ino;
        if rewritten && !trusted {
            return Eligibility::Mismatched;
        }
        // Explicit rewrite consent covers the file inode only. The persistent
        // filesystem IDs, root inode, path, Workspace and native ID remain exact.
        old.ino = now.ino;
        return result(old == now, rewritten);
    }
    let (Ok(mut old), Ok(now)) = (
        serde_json::from_value::<SourceIdentity>(value),
        serde_json::from_str::<SourceIdentity>(&header.legacy_identity),
    ) else {
        return Eligibility::Unrecognized;
    };
    let rewritten = old.ino != now.ino;
    if rewritten && !trusted {
        return Eligibility::Mismatched;
    }
    // Preserve the existing explicit remount-repair path for version 1 pins.
    old.dev = now.dev;
    old.root.dev = now.root.dev;
    old.ino = now.ino;
    result(old == now, rewritten)
}

fn result(matches: bool, rewritten: bool) -> Eligibility {
    match (matches, rewritten) {
        (false, _) => Eligibility::Mismatched,
        (true, true) => Eligibility::TrustedRewrite,
        (true, false) => Eligibility::SameSource,
    }
}
