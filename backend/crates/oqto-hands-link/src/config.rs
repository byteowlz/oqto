//! Owner configuration (ADR-0086 owner decision 2026-11-15). Lease length,
//! renewal and per-machine concurrency are owner values per destination
//! binding, never built-in constants: every field is required, unknown keys
//! and out-of-range values fail closed, and there are no serde defaults.
use anyhow::{Context, Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Protocol bounds the owner values must stay within. These are protocol
/// limits, not defaults: a missing owner value is an error.
pub const LEASE_MS_MIN: u32 = 2_000;
pub const LEASE_MS_MAX: u32 = 10_000;
pub const RENEW_MS_MIN: u32 = 250;
pub const CONCURRENCY_MIN: u8 = 1;
pub const CONCURRENCY_MAX: u8 = 8;
pub const CONFIG_VERSION: u8 = 1;

/// Timing and concurrency the origin owner chose for one machine binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkLimits {
    /// Fixed execution lease offered by each renewal, in milliseconds.
    #[schemars(range(min = 2000, max = 10000))]
    pub lease_ms: u32,
    /// Interval between destination renewal challenges; at most half the lease.
    #[schemars(range(min = 250, max = 5000))]
    pub renew_ms: u32,
    /// Calls that may hold a lease on this machine at the same time.
    #[schemars(range(min = 1, max = 8))]
    pub max_concurrent_calls: u8,
}

impl LinkLimits {
    pub fn validate(&self, context: &str) -> Result<()> {
        ensure!(
            (LEASE_MS_MIN..=LEASE_MS_MAX).contains(&self.lease_ms),
            "{context}: lease_ms {} outside {LEASE_MS_MIN}..={LEASE_MS_MAX}",
            self.lease_ms
        );
        ensure!(
            self.renew_ms >= RENEW_MS_MIN && self.renew_ms <= self.lease_ms / 2,
            "{context}: renew_ms {} must be {RENEW_MS_MIN}..=lease_ms/2 ({})",
            self.renew_ms,
            self.lease_ms / 2
        );
        ensure!(
            (CONCURRENCY_MIN..=CONCURRENCY_MAX).contains(&self.max_concurrent_calls),
            "{context}: max_concurrent_calls {} outside {CONCURRENCY_MIN}..={CONCURRENCY_MAX}",
            self.max_concurrent_calls
        );
        Ok(())
    }
}

/// Machine names are the model-visible `machine` argument values.
pub fn valid_machine_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= 32
        && name != "local"
        && bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn valid_binding_id(id: &str) -> bool {
    valid_machine_name(id)
}

/// Origin (guard) side: `$XDG_CONFIG_HOME/oqt/hands-link.toml`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OriginConfig {
    /// Configuration format version; must be 1.
    pub version: u8,
    /// Named remote machines, keyed by the `machine` argument value.
    pub machines: BTreeMap<String, MachineConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineConfig {
    /// Pairing record created by `oqt_link pair` (state, not config).
    pub pairing: String,
    /// Destination binding to use; the destination owner installed it.
    pub binding: String,
    #[serde(flatten)]
    pub limits: LinkLimits,
}

impl OriginConfig {
    pub fn parse(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text).context("invalid hands-link config")?;
        config.validate()?;
        Ok(config)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let text = read_bounded(path)?;
        Self::parse(&text).with_context(|| format!("loading {}", path.display()))
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == CONFIG_VERSION, "unsupported config version");
        ensure!(self.machines.len() <= 32, "too many machines");
        for (name, machine) in &self.machines {
            ensure!(valid_machine_name(name), "invalid machine name {name:?}");
            ensure!(
                valid_machine_name(&machine.pairing),
                "{name}: invalid pairing name"
            );
            ensure!(
                valid_binding_id(&machine.binding),
                "{name}: invalid binding id"
            );
            machine.limits.validate(name)?;
        }
        Ok(())
    }
}

/// Destination side: the standalone hands host's explicit configuration.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    /// Configuration format version; must be 1.
    pub version: u8,
    /// Socket address to listen on: a tailnet or loopback address, never a wildcard.
    pub listen: String,
    /// Canonical root HTTPS origin printed in pairing QRs.
    pub origin: String,
    /// Private host state (pairing store, claim journal); absolute.
    pub state_dir: PathBuf,
    pub launcher: LauncherConfig,
    /// Owner-installed workspace bindings, keyed by binding id.
    pub bindings: BTreeMap<String, BindingConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LauncherConfig {
    /// The platform's confined hands launcher (`oqt_hands`).
    pub oqt_hands: PathBuf,
    /// The pi-env daemon run inside the hands.
    pub daemon: PathBuf,
    /// Linux: compiled seccomp filter. Must be absent on macOS.
    pub bpf: Option<PathBuf>,
    /// Linux: Landlock shim binary. Must be absent on macOS.
    pub shim: Option<PathBuf>,
    /// Roots the hands may never see (credentials, runtime, authority).
    pub protected_roots: Vec<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BindingConfig {
    /// Native workspace root; the hands' only writable tree.
    pub root: PathBuf,
    /// Destination ceiling for the lease an origin may request.
    #[schemars(range(min = 2000, max = 10000))]
    pub max_lease_ms: u32,
    /// Destination ceiling for concurrent leases per paired device.
    #[schemars(range(min = 1, max = 8))]
    pub max_concurrent_calls: u8,
}

impl HostConfig {
    pub fn parse(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text).context("invalid hands host config")?;
        config.validate()?;
        Ok(config)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let text = read_bounded(path)?;
        Self::parse(&text).with_context(|| format!("loading {}", path.display()))
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == CONFIG_VERSION, "unsupported config version");
        let address: std::net::SocketAddr = self
            .listen
            .parse()
            .context("listen must be an explicit IP:port")?;
        ensure!(
            !address.ip().is_unspecified() && address.port() != 0,
            "listen must name one tailnet or loopback address, never a wildcard"
        );
        ensure!(
            oqto_personal_wire::validate_origin(&self.origin)? == self.origin,
            "origin must be a canonical root HTTPS origin"
        );
        ensure!(self.state_dir.is_absolute(), "state_dir must be absolute");
        let launcher = &self.launcher;
        for path in [&launcher.oqt_hands, &launcher.daemon] {
            ensure!(path.is_absolute(), "launcher paths must be absolute");
        }
        if cfg!(target_os = "linux") {
            ensure!(
                launcher.bpf.as_ref().is_some_and(|p| p.is_absolute())
                    && launcher.shim.as_ref().is_some_and(|p| p.is_absolute()),
                "Linux hands need absolute bpf and shim paths"
            );
        } else if cfg!(target_os = "macos") {
            ensure!(
                launcher.bpf.is_none() && launcher.shim.is_none(),
                "there is no seccomp or Landlock on macOS; remove bpf and shim"
            );
        } else {
            bail!("hands hosts run on Linux or macOS only");
        }
        ensure!(
            !launcher.protected_roots.is_empty() && launcher.protected_roots.len() <= 30,
            "protected_roots required and bounded"
        );
        for root in &launcher.protected_roots {
            ensure!(root.is_absolute(), "protected roots must be absolute");
        }
        ensure!(
            !self.bindings.is_empty() && self.bindings.len() <= 16,
            "one to sixteen bindings"
        );
        for (id, binding) in &self.bindings {
            ensure!(valid_binding_id(id), "invalid binding id {id:?}");
            ensure!(binding.root.is_absolute(), "{id}: root must be absolute");
            ensure!(
                (LEASE_MS_MIN..=LEASE_MS_MAX).contains(&binding.max_lease_ms),
                "{id}: max_lease_ms {} outside {LEASE_MS_MIN}..={LEASE_MS_MAX}",
                binding.max_lease_ms
            );
            ensure!(
                (CONCURRENCY_MIN..=CONCURRENCY_MAX).contains(&binding.max_concurrent_calls),
                "{id}: max_concurrent_calls {} outside {CONCURRENCY_MIN}..={CONCURRENCY_MAX}",
                binding.max_concurrent_calls
            );
            ensure!(
                !binding.root.starts_with(&self.state_dir)
                    && !self.state_dir.starts_with(&binding.root),
                "{id}: root must not overlap the host state"
            );
            for protected in &launcher.protected_roots {
                ensure!(
                    !binding.root.starts_with(protected) && !protected.starts_with(&binding.root),
                    "{id}: root overlaps protected root {}",
                    protected.display()
                );
            }
        }
        Ok(())
    }
}

fn read_bounded(path: &Path) -> Result<String> {
    let metadata =
        std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
    ensure!(
        metadata.is_file() && metadata.len() <= 64 * 1024,
        "config must be a regular file of at most 64 KiB"
    );
    Ok(std::fs::read_to_string(path)?)
}

/// JSON schemas for LSP completion and documentation.
pub fn origin_schema() -> Result<String> {
    Ok(serde_json::to_string_pretty(&schemars::schema_for!(
        OriginConfig
    ))?)
}
pub fn host_schema() -> Result<String> {
    Ok(serde_json::to_string_pretty(&schemars::schema_for!(
        HostConfig
    ))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = include_str!("../examples/hands-link.toml");

    #[test]
    fn example_origin_config_is_valid_and_conservative() -> Result<()> {
        let config = OriginConfig::parse(ORIGIN)?;
        let machine = config
            .machines
            .get("linux-box")
            .context("example machine")?;
        assert_eq!(
            machine.limits,
            LinkLimits {
                lease_ms: 10_000,
                renew_ms: 3_000,
                max_concurrent_calls: 4
            }
        );
        Ok(())
    }

    #[test]
    fn unknown_keys_missing_values_and_ranges_fail_closed() {
        let base = |extra: &str| {
            format!(
                "version = 1\n[machines.linux-box]\npairing = \"linux-box\"\nbinding = \"workspace\"\n{extra}"
            )
        };
        let full = "lease_ms = 10000\nrenew_ms = 3000\nmax_concurrent_calls = 4\n";
        assert!(OriginConfig::parse(&base(full)).is_ok());
        // Missing values are errors, never a hidden default.
        assert!(OriginConfig::parse(&base("lease_ms = 10000\nrenew_ms = 3000\n")).is_err());
        assert!(OriginConfig::parse(&base(&format!("{full}surprise = 1\n"))).is_err());
        for bad in [
            "lease_ms = 10001\nrenew_ms = 3000\nmax_concurrent_calls = 4\n",
            "lease_ms = 1999\nrenew_ms = 500\nmax_concurrent_calls = 4\n",
            "lease_ms = 4000\nrenew_ms = 2001\nmax_concurrent_calls = 4\n",
            "lease_ms = 10000\nrenew_ms = 249\nmax_concurrent_calls = 4\n",
            "lease_ms = 10000\nrenew_ms = 3000\nmax_concurrent_calls = 0\n",
            "lease_ms = 10000\nrenew_ms = 3000\nmax_concurrent_calls = 9\n",
        ] {
            assert!(OriginConfig::parse(&base(bad)).is_err(), "{bad}");
        }
        assert!(OriginConfig::parse(&format!("version = 2\n{}", &base(full)[12..])).is_err());
        assert!(
            OriginConfig::parse(&base(full).replace("machines.linux-box", "machines.local"))
                .is_err()
        );
    }

    #[test]
    fn checked_in_schemas_match_the_types() -> Result<()> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas");
        assert_eq!(
            std::fs::read_to_string(dir.join("hands-link.schema.json"))?.trim(),
            origin_schema()?
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("hands-host.schema.json"))?.trim(),
            host_schema()?
        );
        Ok(())
    }

    #[test]
    fn host_example_parses_on_this_platform() -> Result<()> {
        let name = if cfg!(target_os = "macos") {
            "hands-host.macos.toml"
        } else {
            "hands-host.linux.toml"
        };
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join(name),
        )?;
        let config = HostConfig::parse(&text)?;
        let binding = config.bindings.get("workspace").context("binding")?;
        assert_eq!(
            (binding.max_lease_ms, binding.max_concurrent_calls),
            (10_000, 4)
        );
        // Wildcard listeners and unknown keys are refused.
        assert!(HostConfig::parse(&text.replace("127.0.0.1:7443", "0.0.0.0:7443")).is_err());
        assert!(HostConfig::parse(&format!("{text}\nextra = 1\n")).is_err());
        Ok(())
    }
}
