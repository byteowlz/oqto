//! Fail-closed container-placement prerequisite doctor.
//!
//! Read-only probes that verify a host can run rootless-Podman Workspace
//! placements. Each probe yields a stable `ContainerCheck` (id/severity/status/
//! evidence/remediation) suitable for `oqtoctl doctor --profile container
//! --strict --json`. The CLI owns human rendering and `--apply` safe
//! remediation; this module only inspects.

use crate::CommandRunner;
use serde::Serialize;
use std::path::PathBuf;

/// Severity vocabulary shared with the install-contract doctor. Mirrors
/// `oqto_provisioning::CheckSeverity` without pulling a cross-crate dep into
/// the placement crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckSeverity {
    Error,
    Warn,
    Info,
}

/// Minimum Podman version for rootless userns=auto placements. Podman <4 lacks
/// reliable userns=auto + cgroup delegation; ubuntu-builder's 3.4 must fail.
pub const MIN_PODMAN_VERSION: (u32, u32, u32) = (4, 0, 0);

/// Minimum contiguous subuid/subgid range required per backend user. userns=auto
/// allocates a disjoint range per container; a single 65536 block is the floor.
pub const MIN_SUBID_COUNT: u64 = 65536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Fail,
    Skip,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContainerCheck {
    /// Stable identifier (never reordered/renamed across releases).
    pub id: &'static str,
    pub severity: CheckSeverity,
    pub status: CheckStatus,
    pub description: &'static str,
    /// Observed value / diagnostic evidence.
    pub evidence: String,
    /// Actionable operator remediation.
    pub remediation: String,
}

impl ContainerCheck {
    fn pass(id: &'static str, description: &'static str, evidence: impl Into<String>) -> Self {
        Self {
            id,
            severity: CheckSeverity::Info,
            status: CheckStatus::Pass,
            description,
            evidence: evidence.into(),
            remediation: String::new(),
        }
    }

    fn fail(
        id: &'static str,
        severity: CheckSeverity,
        description: &'static str,
        evidence: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            id,
            severity,
            status: CheckStatus::Fail,
            description,
            evidence: evidence.into(),
            remediation: remediation.into(),
        }
    }

    fn skip(id: &'static str, description: &'static str, reason: impl Into<String>) -> Self {
        Self {
            id,
            severity: CheckSeverity::Info,
            status: CheckStatus::Skip,
            description,
            evidence: reason.into(),
            remediation: String::new(),
        }
    }

    /// True when this check blocks placements under `--strict`.
    pub fn is_blocker(&self) -> bool {
        self.status == CheckStatus::Fail && self.severity == CheckSeverity::Error
    }
}

#[derive(Clone, Debug, Default)]
pub struct ContainerDoctorOptions {
    /// Backend service user (the one running `oqto serve`). Defaults to the
    /// current user when omitted.
    pub user: Option<String>,
    /// Configured workspace image reference, e.g.
    /// `ghcr.io/byteowlz/oqto-workspace:0.6.0`.
    pub image: Option<String>,
    /// Configured image digest pin (`sha256:...`).
    pub image_digest: Option<String>,
    /// State/runtime roots to verify.
    pub state_root: Option<PathBuf>,
    pub runtime_root: Option<PathBuf>,
    /// Override the podman binary (testing).
    pub podman_binary: Option<String>,
    /// Override /etc/subuid path (testing).
    pub subuid_path: Option<PathBuf>,
    /// Override /etc/subgid path (testing).
    pub subgid_path: Option<PathBuf>,
    /// Override the cgroup v2 mount root (testing).
    pub cgroup_root: Option<PathBuf>,
}

impl ContainerDoctorOptions {
    fn subuid_path(&self) -> PathBuf {
        self.subuid_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("/etc/subuid"))
    }
    fn subgid_path(&self) -> PathBuf {
        self.subgid_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("/etc/subgid"))
    }
    fn cgroup_root(&self) -> PathBuf {
        self.cgroup_root
            .clone()
            .unwrap_or_else(|| PathBuf::from("/sys/fs/cgroup"))
    }
}

/// Run all container-placement probes and return the results in a stable order.
pub async fn run_container_doctor<R: CommandRunner>(
    opts: &ContainerDoctorOptions,
    runner: &R,
) -> Vec<ContainerCheck> {
    let mut checks = Vec::new();
    let podman = opts
        .podman_binary
        .clone()
        .unwrap_or_else(|| "podman".to_string());

    // --- Podman install + version + rootless + cgroup ---
    let info = podman_info(runner, &podman).await;
    checks.push(podman_version_check(&podman, &info));
    checks.push(podman_rootless_check(&info));
    checks.push(podman_cgroup_check(&info));

    // --- subuid / subgid ---
    let user = opts
        .user
        .clone()
        .or_else(|| {
            std::env::var("USER")
                .ok()
                .or_else(|| std::env::var("LOGNAME").ok())
        })
        .unwrap_or_default();
    checks.push(subid_check(
        "subuid.range",
        "Disjoint subuid range for userns=auto",
        &opts.subuid_path(),
        &user,
    ));
    checks.push(subid_check(
        "subgid.range",
        "Disjoint subgid range for userns=auto",
        &opts.subgid_path(),
        &user,
    ));

    // --- linger ---
    checks.push(linger_check(&user).await);

    // --- cgroup controller delegation ---
    checks.push(cgroup_delegation_check(&user, &opts.cgroup_root()));

    // --- image reference + live attestation ---
    checks.push(image_reference_check(opts));
    checks.push(image_attestation_check(opts, runner).await);

    // --- roots ---
    checks.push(root_check(
        "roots.state",
        "Per-workspace durable state root",
        opts.state_root.as_deref(),
    ));
    checks.push(root_check(
        "roots.runtime",
        "Per-workspace runtime socket root",
        opts.runtime_root.as_deref(),
    ));

    checks
}

async fn podman_info<R: CommandRunner>(runner: &R, podman: &str) -> Option<serde_json::Value> {
    use std::ffi::OsString;
    let out = runner
        .run(
            podman,
            &[
                OsString::from("info"),
                OsString::from("--format"),
                OsString::from("json"),
            ],
        )
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()
}

fn podman_version_check(podman: &str, info: &Option<serde_json::Value>) -> ContainerCheck {
    let Some(info) = info else {
        return ContainerCheck::fail(
            "podman.installed",
            CheckSeverity::Error,
            "Podman installed and reachable",
            format!("`{podman} info` did not return usable output"),
            "Install rootless Podman >= 4.0 (e.g. `apt-get install podman` on Ubuntu 22.04+)",
        );
    };
    let version = info
        .pointer("/version/Version")
        .or_else(|| info.pointer("/host/version/Version"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    match parse_semver(version) {
        Some(parsed) if parsed >= MIN_PODMAN_VERSION => ContainerCheck::pass(
            "podman.version",
            "Podman minimum version for userns=auto placements",
            format!(
                "{version} >= {}.{}.{}",
                MIN_PODMAN_VERSION.0, MIN_PODMAN_VERSION.1, MIN_PODMAN_VERSION.2
            ),
        ),
        Some(parsed) => ContainerCheck::fail(
            "podman.version",
            CheckSeverity::Error,
            "Podman minimum version for userns=auto placements",
            format!(
                "Podman {version} ({}.{}.{} < required {}.{}.{}); userns=auto and cgroup delegation are unreliable",
                parsed.0,
                parsed.1,
                parsed.2,
                MIN_PODMAN_VERSION.0,
                MIN_PODMAN_VERSION.1,
                MIN_PODMAN_VERSION.2
            ),
            "Upgrade Podman to >= 4.0 (Ubuntu 24.04 ships 4.x; on 22.04 use the OBS/devel:kubic repo or a newer runtime image)",
        ),
        None => ContainerCheck::fail(
            "podman.version",
            CheckSeverity::Warn,
            "Podman minimum version for userns=auto placements",
            format!("could not parse Podman version '{version}'"),
            "Ensure podman --version reports a semver like `4.3.1`",
        ),
    }
}

fn podman_rootless_check(info: &Option<serde_json::Value>) -> ContainerCheck {
    let Some(info) = info else {
        return ContainerCheck::skip(
            "podman.rootless",
            "Rootless Podman mode",
            "podman info unavailable (see podman.installed)",
        );
    };
    match info
        .pointer("/host/security/rootless")
        .and_then(|v| v.as_bool())
    {
        Some(true) => {
            ContainerCheck::pass("podman.rootless", "Rootless Podman mode", "rootless=true")
        }
        Some(false) => ContainerCheck::fail(
            "podman.rootless",
            CheckSeverity::Error,
            "Rootless Podman mode",
            "podman reports rootless=false",
            "Do not run the backend as root. Run as the service user; configure /etc/subuid+subgid and rootless storage (podman system setup).",
        ),
        None => ContainerCheck::skip(
            "podman.rootless",
            "Rootless Podman mode",
            "rootless field absent in podman info",
        ),
    }
}

fn podman_cgroup_check(info: &Option<serde_json::Value>) -> ContainerCheck {
    let Some(info) = info else {
        return ContainerCheck::skip(
            "podman.cgroup",
            "cgroup v2 + systemd manager",
            "podman info unavailable",
        );
    };
    let version = info.pointer("/host/cgroupVersion").and_then(|v| v.as_str());
    let manager = info.pointer("/host/cgroupManager").and_then(|v| v.as_str());
    match (version, manager) {
        (Some("v2"), Some("systemd")) => ContainerCheck::pass(
            "podman.cgroup",
            "cgroup v2 + systemd manager",
            "cgroupVersion=v2 cgroupManager=systemd",
        ),
        (v, m) => ContainerCheck::fail(
            "podman.cgroup",
            CheckSeverity::Error,
            "cgroup v2 + systemd manager",
            format!(
                "cgroupVersion={} cgroupManager={} (need v2 + systemd)",
                v.unwrap_or("?"),
                m.unwrap_or("?")
            ),
            "Boot with systemd as init and cgroup v2 (systemd.unified_cgroup_hierarchy=1); Podman rootless resource limits require systemd cgroup manager.",
        ),
    }
}

fn subid_check(
    id: &'static str,
    description: &'static str,
    path: &std::path::Path,
    user: &str,
) -> ContainerCheck {
    let Ok(content) = std::fs::read_to_string(path) else {
        return ContainerCheck::fail(
            id,
            CheckSeverity::Error,
            description,
            format!("{} not readable", path.display()),
            format!(
                "Provision the backend user with `usermod --add-subuids ... --add-subgids ... {user}` or `podman system setup`"
            ),
        );
    };
    let entry = content.lines().find_map(|line| {
        let mut parts = line.split(':');
        let name = parts.next()?;
        let start = parts.next()?.parse::<u64>().ok()?;
        let count = parts.next()?.parse::<u64>().ok()?;
        (name == user).then_some((start, count))
    });
    match entry {
        Some((_start, count)) if count >= MIN_SUBID_COUNT => ContainerCheck::pass(
            id,
            description,
            format!("{user}: {count} ids (>= {MIN_SUBID_COUNT})"),
        ),
        Some((_start, count)) => ContainerCheck::fail(
            id,
            CheckSeverity::Error,
            description,
            format!("{user}: only {count} ids (need >= {MIN_SUBID_COUNT})"),
            format!(
                "Increase the range: `usermod --add-subuids {start}-{end} --add-subgids ... {user}`",
                start = 1_000_000,
                end = 1_000_000 + MIN_SUBID_COUNT - 1
            ),
        ),
        None => ContainerCheck::fail(
            id,
            CheckSeverity::Error,
            description,
            format!("no entry for '{user}' in {}", path.display()),
            format!(
                "Add a subuid/subgid range for {user} (rootless podman setup or `usermod --add-sub-uids/sub-gids`)"
            ),
        ),
    }
}

async fn linger_check(user: &str) -> ContainerCheck {
    // `loginctl show-user` is the authoritative source; polkit may allow the
    // unprivileged user to query themselves.
    let out = tokio::process::Command::new("loginctl")
        .args(["show-user", user, "--value", "-p", "Linger"])
        .output()
        .await;
    let Ok(out) = out else {
        return ContainerCheck::skip(
            "linger.enabled",
            "systemd linger for backend user",
            "loginctl not available",
        );
    };
    if !out.status.success() {
        return ContainerCheck::fail(
            "linger.enabled",
            CheckSeverity::Error,
            "systemd linger for backend user",
            format!("loginctl show-user {user} failed"),
            format!(
                "Enable linger: `sudo loginctl enable-linger {user}` (or `oqtoctl doctor --profile container --apply`)"
            ),
        );
    }
    let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if val == "yes" {
        ContainerCheck::pass(
            "linger.enabled",
            "systemd linger for backend user",
            format!("{user}: linger=yes"),
        )
    } else {
        ContainerCheck::fail(
            "linger.enabled",
            CheckSeverity::Error,
            "systemd linger for backend user",
            format!("{user}: linger={val}"),
            format!(
                "Enable linger: `sudo loginctl enable-linger {user}` so the backend survives logout and owns a cgroup"
            ),
        )
    }
}

fn cgroup_delegation_check(user: &str, cgroup_root: &std::path::Path) -> ContainerCheck {
    // The backend user's slice must have cpu/memory/pids delegated via
    // systemd (persistent), not a transient subtree_control write.
    let uid = lookup_uid(user);
    let Some(uid) = uid else {
        return ContainerCheck::skip(
            "cgroup.delegation",
            "cpu/memory/pids cgroup delegation",
            format!("could not resolve uid for '{user}'"),
        );
    };
    let user_slice = cgroup_root.join(format!("user.slice/user-{uid}.slice"));
    let Ok(controllers) = read_cgroup_field(&user_slice, "cgroup.controllers") else {
        return ContainerCheck::fail(
            "cgroup.delegation",
            CheckSeverity::Error,
            "cpu/memory/pids cgroup delegation",
            format!("no user cgroup slice at {}", user_slice.display()),
            "Ensure the backend user is logged in / has a user session (linger) so systemd creates user-{uid}.slice",
        );
    };
    let required = ["cpu", "memory", "pids"];
    let missing: Vec<&str> = required
        .iter()
        .filter(|c| !controllers.split_whitespace().any(|x| x == **c))
        .copied()
        .collect();
    if missing.is_empty() {
        ContainerCheck::pass(
            "cgroup.delegation",
            "cpu/memory/pids cgroup delegation",
            format!("user-{uid}.slice controllers: {controllers}"),
        )
    } else {
        ContainerCheck::fail(
            "cgroup.delegation",
            CheckSeverity::Error,
            "cpu/memory/pids cgroup delegation",
            format!(
                "user-{uid}.slice missing controllers: {} (have: {controllers})",
                missing.join(" ")
            ),
            "Delegate controllers persistently via systemd: `systemctl --user edit user@.service` adding [Service]/Delegate=cpu memory pids, or enable `systemd --user` delegation. Do NOT rely on transient cgroup.subtree_control writes.",
        )
    }
}

fn image_reference_check(opts: &ContainerDoctorOptions) -> ContainerCheck {
    let Some(image) = &opts.image else {
        return ContainerCheck::skip(
            "image.attestation",
            "Workspace image version/digest attestation",
            "no placement.image configured",
        );
    };
    if image.ends_with(":latest") || !image.contains(':') {
        return ContainerCheck::fail(
            "image.reference",
            CheckSeverity::Error,
            "Workspace image is exact version/digest (not latest)",
            format!("image={image}"),
            "Pin an exact release tag, e.g. ghcr.io/byteowlz/oqto-workspace:<version>, and set placement.image_digest for production",
        );
    }
    if opts.image_digest.is_none() && !is_local_ref(image) {
        // Warn (not error): deploy can proceed on version match alone, but
        // production should pin a digest to reject silent drift.
        return ContainerCheck {
            id: "image.digest",
            severity: CheckSeverity::Warn,
            status: CheckStatus::Pass,
            description: "Workspace image digest pin",
            evidence: format!("image={image} has no placement.image_digest"),
            remediation:
                "Pin placement.image_digest (sha256:...) from the release metadata to detect drift"
                    .to_string(),
        };
    }
    ContainerCheck::pass(
        "image.reference",
        "Workspace image is exact version/digest (not latest)",
        format!(
            "image={image}{}",
            opts.image_digest
                .as_deref()
                .map(|d| format!(" digest={d}"))
                .unwrap_or_default()
        ),
    )
}

/// Live attestation: `podman inspect` the configured image and verify the
/// `io.oqto.version` label matches the backend release version (and the
/// configured digest pin, if any). Mirrors the placement attestation seam so
/// the doctor and the backend agree on what "compatible" means.
async fn image_attestation_check<R: CommandRunner>(
    opts: &ContainerDoctorOptions,
    runner: &R,
) -> ContainerCheck {
    use std::ffi::OsString;
    const EXPECTED: &str = env!("CARGO_PKG_VERSION");
    let Some(image) = &opts.image else {
        return ContainerCheck::skip(
            "image.attestation",
            "Live image label/digest attestation",
            "no placement.image configured",
        );
    };
    let podman = opts
        .podman_binary
        .clone()
        .unwrap_or_else(|| "podman".to_string());
    let out = runner
        .run(
            &podman,
            &[
                OsString::from("inspect"),
                OsString::from("--format"),
                OsString::from("json"),
                OsString::from(image),
            ],
        )
        .await;
    let out = match out {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            return ContainerCheck::fail(
                "image.attestation",
                CheckSeverity::Error,
                "Live image label/digest attestation",
                format!(
                    "podman inspect failed: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ),
                format!("Pull the image first: `podman pull {image}`"),
            );
        }
        Err(error) => {
            return ContainerCheck::fail(
                "image.attestation",
                CheckSeverity::Error,
                "Live image label/digest attestation",
                format!("could not run podman inspect: {error}"),
                "Ensure podman is installed and the backend user can run it rootless",
            );
        }
    };
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct InspectImage {
        #[serde(default)]
        digest: Option<String>,
        #[serde(default)]
        labels: Option<std::collections::BTreeMap<String, String>>,
    }
    let parsed: Vec<InspectImage> = match serde_json::from_slice(&out.stdout) {
        Ok(v) => v,
        Err(error) => {
            return ContainerCheck::fail(
                "image.attestation",
                CheckSeverity::Error,
                "Live image label/digest attestation",
                format!("could not parse podman inspect output: {error}"),
                "Ensure podman --version is supported and returns JSON for --format json",
            );
        }
    };
    let Some(entry) = parsed.into_iter().next() else {
        return ContainerCheck::fail(
            "image.attestation",
            CheckSeverity::Error,
            "Live image label/digest attestation",
            format!("podman inspect returned no image for {image}"),
            format!("Pull the image: `podman pull {image}`"),
        );
    };
    let labels = entry.labels.unwrap_or_default();
    let digest = entry.digest.unwrap_or_default();
    match labels.get(crate::IMAGE_VERSION_LABEL) {
        Some(v) if v == EXPECTED => {}
        Some(v) => {
            return ContainerCheck::fail(
                "image.attestation",
                CheckSeverity::Error,
                "Live image label/digest attestation",
                format!("{image} io.oqto.version={v} != backend {EXPECTED}"),
                format!("Use ghcr.io/byteowlz/oqto-workspace:{EXPECTED}"),
            );
        }
        None => {
            return ContainerCheck::fail(
                "image.attestation",
                CheckSeverity::Error,
                "Live image label/digest attestation",
                format!(
                    "{image} has no {} label (unattested :dev?)",
                    crate::IMAGE_VERSION_LABEL
                ),
                "Use a release image built with deploy/workspace/build.sh --release",
            );
        }
    }
    if let Some(expected_digest) = &opts.image_digest
        && !expected_digest.is_empty()
        && expected_digest != &digest
    {
        return ContainerCheck::fail(
            "image.attestation",
            CheckSeverity::Error,
            "Live image label/digest attestation",
            format!("digest drift: configured {expected_digest} != resolved {digest}"),
            "Update placement.image_digest or repull the pinned image",
        );
    }
    ContainerCheck::pass(
        "image.attestation",
        "Live image label/digest attestation",
        format!("{image} io.oqto.version={EXPECTED} digest={digest}"),
    )
}

fn root_check(
    id: &'static str,
    description: &'static str,
    root: Option<&std::path::Path>,
) -> ContainerCheck {
    let Some(root) = root else {
        return ContainerCheck::skip(id, description, "not configured");
    };
    match std::fs::metadata(root) {
        Ok(meta) if meta.is_dir() => {
            ContainerCheck::pass(id, description, format!("{} (dir)", root.display()))
        }
        Ok(_) => ContainerCheck::fail(
            id,
            CheckSeverity::Error,
            description,
            format!("{} exists but is not a directory", root.display()),
            format!(
                "Remove the non-directory path and let Oqto create it: `{}`",
                root.display()
            ),
        ),
        Err(_) => ContainerCheck::fail(
            id,
            CheckSeverity::Warn,
            description,
            format!("{} does not exist", root.display()),
            format!(
                "Create it: `install -d -m 0750 {root}` (or `oqtoctl doctor --profile container --apply`)",
                root = root.display()
            ),
        ),
    }
}

// ---------- helpers ----------

fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches('v');
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts
        .next()
        .unwrap_or("0")
        .split('-')
        .next()?
        .parse()
        .ok()?;
    Some((major, minor, patch))
}

fn lookup_uid(user: &str) -> Option<u32> {
    if user.is_empty() {
        return None;
    }
    // Prefer `getent passwd <user>` (respects NSS/SSSD/LDAP); parse uid field 3.
    let via_getent = std::process::Command::new("getent")
        .args(["passwd", user])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()?
                .split(':')
                .nth(2)?
                .parse()
                .ok()
        });
    if via_getent.is_some() {
        return via_getent;
    }
    // Fall back to a local /etc/passwd scan (no NSS), then to the current
    // effective uid when the requested user matches $USER.
    let via_passwd = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|content| {
            content.lines().find_map(|line| {
                let mut parts = line.split(':');
                let name = parts.next()?;
                let _passwd = parts.next()?;
                let uid = parts.next()?.parse().ok()?;
                (name == user).then_some(uid)
            })
        });
    if via_passwd.is_some() {
        return via_passwd;
    }
    None
}

fn read_cgroup_field(slice: &std::path::Path, field: &str) -> std::io::Result<String> {
    std::fs::read_to_string(slice.join(field))
}

/// An image reference is "local" when it carries no registry host (localhost/,
/// bare name). Mirrors the backend placement attestation policy.
fn is_local_ref(image: &str) -> bool {
    let name = image.split(':').next().unwrap_or(image);
    if let Some(first) = name.split('/').next() {
        if first.contains('.') || first.contains(':') {
            return false;
        }
        if first == "localhost" {
            return true;
        }
    }
    !name.contains('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TokioCommandRunner;

    #[test]
    fn parse_semver_handles_dev_and_rc_suffixes() {
        assert_eq!(parse_semver("4.3.1"), Some((4, 3, 1)));
        assert_eq!(parse_semver("4.3"), Some((4, 3, 0)));
        assert_eq!(parse_semver("v5.0.0"), Some((5, 0, 0)));
        assert_eq!(parse_semver("4.9.12-dev"), Some((4, 9, 12)));
        assert_eq!(parse_semver("garbage"), None);
    }

    #[test]
    fn subid_check_passes_with_sufficient_range() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("subuid");
        std::fs::write(&path, "other:100000:65536\noqto:200000:131072\n").unwrap();
        let check = subid_check("subuid.range", "x", &path, "oqto");
        assert_eq!(check.status, CheckStatus::Pass);
        assert_eq!(check.id, "subuid.range");
    }

    #[test]
    fn subid_check_fails_on_short_range() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("subuid");
        std::fs::write(&path, "oqto:200000:100\n").unwrap();
        let check = subid_check("subuid.range", "x", &path, "oqto");
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.severity == CheckSeverity::Error);
        assert!(check.evidence.contains("100"));
    }

    #[test]
    fn subid_check_fails_on_missing_user() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("subuid");
        std::fs::write(&path, "someoneelse:100000:65536\n").unwrap();
        let check = subid_check("subuid.range", "x", &path, "oqto");
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.evidence.contains("no entry"));
    }

    #[test]
    fn podman_version_check_rejects_3_4() {
        let info = serde_json::json!({"host": {"version": {"Version": "3.4.4"}}});
        let check = podman_version_check("podman", &Some(info));
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.evidence.contains("3.4.4"));
    }

    #[test]
    fn podman_version_check_accepts_4_x() {
        let info = serde_json::json!({"host": {"version": {"Version": "4.9.4"}}});
        let check = podman_version_check("podman", &Some(info));
        assert_eq!(check.status, CheckStatus::Pass);
    }

    #[test]
    fn podman_cgroup_check_requires_systemd_v2() {
        let good = serde_json::json!({"host": {"cgroupVersion": "v2", "cgroupManager": "systemd"}});
        assert_eq!(podman_cgroup_check(&Some(good)).status, CheckStatus::Pass);
        let bad = serde_json::json!({"host": {"cgroupVersion": "v1", "cgroupManager": "cgroupfs"}});
        let check = podman_cgroup_check(&Some(bad));
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.evidence.contains("v1"));
    }

    #[test]
    fn image_check_rejects_latest_tag() {
        let opts = ContainerDoctorOptions {
            image: Some("ghcr.io/byteowlz/oqto-workspace:latest".to_string()),
            ..Default::default()
        };
        let check = image_reference_check(&opts);
        assert_eq!(check.status, CheckStatus::Fail);
        assert_eq!(check.id, "image.reference");
    }

    #[test]
    fn image_check_warns_on_missing_digest_for_registry_ref() {
        let opts = ContainerDoctorOptions {
            image: Some("ghcr.io/byteowlz/oqto-workspace:0.6.0".to_string()),
            ..Default::default()
        };
        let check = image_reference_check(&opts);
        assert_eq!(check.status, CheckStatus::Pass);
        assert_eq!(check.severity, CheckSeverity::Warn);
        assert_eq!(check.id, "image.digest");
    }

    #[test]
    fn image_check_passes_local_dev_without_digest() {
        let opts = ContainerDoctorOptions {
            image: Some("localhost/oqto-workspace:dev".to_string()),
            ..Default::default()
        };
        let check = image_reference_check(&opts);
        assert_eq!(check.status, CheckStatus::Pass);
        assert_eq!(check.id, "image.reference");
    }

    #[test]
    fn root_check_flags_missing_dir_as_warn() {
        let check = root_check(
            "roots.state",
            "x",
            Some(std::path::Path::new("/nonexistent/oqto-state-root-xyz")),
        );
        assert_eq!(check.status, CheckStatus::Fail);
        assert_eq!(check.severity, CheckSeverity::Warn);
    }

    #[tokio::test]
    async fn doctor_runs_against_real_podman_or_skips() {
        // Smoke: against the real host (CI/dev), the doctor must not panic and
        // must emit the stable check IDs in order.
        let runner = TokioCommandRunner;
        let opts = ContainerDoctorOptions {
            image: Some("localhost/oqto-workspace:dev".to_string()),
            ..Default::default()
        };
        let checks = run_container_doctor(&opts, &runner).await;
        let ids: Vec<&str> = checks.iter().map(|c| c.id).collect();
        assert!(ids.starts_with(&["podman.version", "podman.rootless", "podman.cgroup"]));
        assert!(ids.contains(&"subuid.range"));
        assert!(ids.contains(&"linger.enabled"));
        assert!(ids.contains(&"image.reference"));
    }
}
