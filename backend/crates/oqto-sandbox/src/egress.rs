//! Level-2 egress capture for `NetworkMode::Proxy`.
//!
//! When an agent runs under `NetworkMode::Proxy`, it is placed inside a
//! dedicated network namespace connected to the host by a single veth pair.
//! The host deliberately does **not** enable forwarding or masquerade for that
//! veth, so the namespace can reach nothing except the host-side veth address.
//! A transparent proxy listening on the host-side veth address is therefore the
//! only possible egress path. nftables rules inside the namespace transparently
//! redirect the agent's TCP to that proxy, force DNS to the proxy resolver, and
//! drop everything else.
//!
//! Capture guarantee (within a shared kernel): because no route off the host
//! veth exists, an agent that bypasses the nftables redirect (e.g. a raw
//! socket) still cannot reach the internet -- the only reachable peer is the
//! host veth, where only the proxy and resolver sockets listen. This holds
//! against a misbehaving in-namespace process; it does not hold against a
//! kernel LPE that can rewrite host routing (that is the microVM tier's job).
//! See `docs/active/design/20260609-isolation-tiers-and-egress.md`.
//!
//! This module is intentionally split:
//! - everything except [`EgressPlan::apply`] / [`EgressPlan::teardown`] is pure
//!   (it computes layout and emits `ip`/`nft` argv + ruleset text), so it is
//!   exhaustively unit-testable without privileges;
//! - `apply`/`teardown` execute those commands and require `CAP_NET_ADMIN`.

use std::io::Write;
use std::net::Ipv4Addr;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use log::{debug, info, warn};

use crate::config::{NetworkConfig, NetworkMode};

/// Base of the address pool: `10.0.0.0/8`, tiled into `/30` point-to-point links.
const EGRESS_BASE: u32 = 0x0A00_0000;

/// Highest usable `/30` block index that stays within `10.0.0.0/8`.
/// `10/8` holds `2^24` addresses = `2^22` `/30` blocks.
const MAX_INDEX: u32 = (1 << 22) - 1;

/// Prefix length of each per-session point-to-point link.
const PREFIX: u8 = 30;

/// Where the transparent proxy and DNS resolver listen on the host side of the
/// veth. Ports are host-configured; the bind address is the allocated host
/// veth IP (the agent never learns the real upstream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EgressProxy {
    /// host-side port of the transparent TCP proxy
    pub tcp_port: u16,
    /// host-side port of the DNS resolver
    pub dns_port: u16,
}

/// Fully resolved layout for one proxy-mode network namespace.
///
/// All names fit within `IFNAMSIZ` (15) for the index range we allow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressPlan {
    /// `/30` block index; selects the subnet and the namespace/veth names
    pub index: u32,
    /// network namespace name (`ip netns` identity)
    pub netns: String,
    /// host-side veth interface name
    pub host_veth: String,
    /// namespace-side veth interface name
    pub guest_veth: String,
    /// host-side veth address; the proxy/resolver bind here
    pub host_ip: Ipv4Addr,
    /// namespace-side veth address; the agent's only local address
    pub guest_ip: Ipv4Addr,
    /// point-to-point prefix length
    pub prefix: u8,
    /// transparent proxy ports on `host_ip`
    pub proxy: EgressProxy,
    /// domains the proxy is expected to allow (passed through for the proxy's
    /// own ACL; not enforced by nftables, which only does capture)
    pub allow_domains: Vec<String>,
}

/// Compute the `(host_ip, guest_ip)` pair for a `/30` block index.
fn subnet_for_index(index: u32) -> (Ipv4Addr, Ipv4Addr) {
    let net = EGRESS_BASE | (index << 2);
    (Ipv4Addr::from(net + 1), Ipv4Addr::from(net + 2))
}

impl EgressPlan {
    /// Build a plan for an allocated `/30` block index.
    ///
    /// The caller is responsible for handing out a unique `index` per live
    /// namespace; reusing a live index would collide on subnet and names.
    pub fn new(index: u32, proxy: EgressProxy, allow_domains: Vec<String>) -> Result<Self> {
        if index > MAX_INDEX {
            bail!("egress subnet index {index} exceeds max {MAX_INDEX} (10.0.0.0/8 exhausted)");
        }
        if proxy.tcp_port == 0 || proxy.dns_port == 0 {
            bail!("egress proxy tcp/dns ports must be non-zero");
        }
        let (host_ip, guest_ip) = subnet_for_index(index);
        Ok(Self {
            index,
            netns: format!("oqto-egr-{index}"),
            host_veth: format!("oqe-h{index}"),
            guest_veth: format!("oqe-g{index}"),
            host_ip,
            guest_ip,
            prefix: PREFIX,
            proxy,
            allow_domains,
        })
    }

    /// Resolve an [`EgressPlan`] from a [`NetworkConfig`], or `None` when the
    /// mode needs no namespace (`Open`/`Isolated`).
    ///
    /// Fails closed: `Proxy` without a configured proxy TCP port is an error,
    /// never a silent fall-through to open networking.
    pub fn from_network_config(cfg: &NetworkConfig, index: u32) -> Result<Option<Self>> {
        match cfg.mode {
            NetworkMode::Open | NetworkMode::Isolated => Ok(None),
            NetworkMode::Proxy => {
                let tcp_port = cfg.proxy_tcp_port.context(
                    "NetworkMode::Proxy requires network.proxy_tcp_port; refusing to launch \
                     (fail-closed)",
                )?;
                let dns_port = cfg.proxy_dns_port.unwrap_or(tcp_port);
                let plan = Self::new(
                    index,
                    EgressProxy { tcp_port, dns_port },
                    cfg.allow_domains.clone(),
                )?;
                Ok(Some(plan))
            }
        }
    }

    /// `ip` commands that create and configure the namespace + veth pair.
    /// Ordered; run as root before launching the agent.
    pub fn setup_commands(&self) -> Vec<Vec<String>> {
        let ns = &self.netns;
        let h = &self.host_veth;
        let g = &self.guest_veth;
        let host_cidr = format!("{}/{}", self.host_ip, self.prefix);
        let guest_cidr = format!("{}/{}", self.guest_ip, self.prefix);
        let argv = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
        vec![
            argv(&format!("ip netns add {ns}")),
            argv(&format!("ip link add {h} type veth peer name {g}")),
            argv(&format!("ip link set {g} netns {ns}")),
            argv(&format!("ip addr add {host_cidr} dev {h}")),
            argv(&format!("ip link set {h} up")),
            argv(&format!(
                "ip netns exec {ns} ip addr add {guest_cidr} dev {g}"
            )),
            argv(&format!("ip netns exec {ns} ip link set {g} up")),
            argv(&format!("ip netns exec {ns} ip link set lo up")),
            argv(&format!(
                "ip netns exec {ns} ip route add default via {}",
                self.host_ip
            )),
        ]
    }

    /// The nftables ruleset (for `nft -f -`) applied **inside** the namespace.
    ///
    /// - `nat output`: redirect DNS (53) to the resolver and all other TCP to
    ///   the transparent proxy, both on the host veth IP.
    /// - `filter output`: default-drop; only traffic now destined to the host
    ///   veth (i.e. already redirected) plus loopback/established is allowed.
    ///
    /// There is deliberately no `masquerade`/`snat`/forward rule: the host does
    /// not route the namespace onward, so non-redirected egress is dropped both
    /// by this policy and by the absence of any route off the veth.
    pub fn nft_ruleset(&self) -> String {
        let host = self.host_ip;
        let tcp = self.proxy.tcp_port;
        let dns = self.proxy.dns_port;
        format!(
            "table inet oqto_egress {{\n\
             \x20   chain output_nat {{\n\
             \x20       type nat hook output priority -100; policy accept;\n\
             \x20       ip daddr {host} tcp dport {tcp} accept\n\
             \x20       ip daddr {host} udp dport {dns} accept\n\
             \x20       udp dport 53 dnat ip to {host}:{dns}\n\
             \x20       tcp dport 53 dnat ip to {host}:{dns}\n\
             \x20       meta l4proto tcp dnat ip to {host}:{tcp}\n\
             \x20   }}\n\
             \x20   chain output_filter {{\n\
             \x20       type filter hook output priority 0; policy drop;\n\
             \x20       oif \"lo\" accept\n\
             \x20       ct state established,related accept\n\
             \x20       ip daddr {host} accept\n\
             \x20   }}\n\
             }}\n"
        )
    }

    /// Argv that pipes [`nft_ruleset`](Self::nft_ruleset) into `nft` inside the
    /// namespace (ruleset supplied on stdin).
    pub fn nft_command(&self) -> Vec<String> {
        vec![
            "ip".into(),
            "netns".into(),
            "exec".into(),
            self.netns.clone(),
            "nft".into(),
            "-f".into(),
            "-".into(),
        ]
    }

    /// `ip` commands that tear down the namespace and veth. Deleting the
    /// namespace removes its veth end; the host end is removed explicitly in
    /// case setup failed partway.
    pub fn teardown_commands(&self) -> Vec<Vec<String>> {
        let argv = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
        vec![
            argv(&format!("ip netns del {}", self.netns)),
            argv(&format!("ip link del {}", self.host_veth)),
        ]
    }

    /// Wrap an inner command (e.g. the full `bwrap ...` argv) so it executes
    /// inside the namespace. The agent must NOT additionally `--unshare-net`,
    /// which would replace this configured namespace with an empty one.
    pub fn wrap_command(&self, inner: &[String]) -> Vec<String> {
        let mut out = vec![
            "ip".to_string(),
            "netns".to_string(),
            "exec".to_string(),
            self.netns.clone(),
        ];
        out.extend_from_slice(inner);
        out
    }

    /// Create and configure the namespace. Requires `CAP_NET_ADMIN`.
    ///
    /// On any failure this attempts teardown so a half-built namespace is not
    /// left behind (which would leak the subnet index).
    pub fn apply(&self) -> Result<()> {
        if !privileged() {
            bail!("egress apply requires CAP_NET_ADMIN (run as root / via the runner)");
        }
        info!(
            "egress: creating namespace {} ({} <-> {}) -> proxy {}:{}",
            self.netns, self.host_ip, self.guest_ip, self.host_ip, self.proxy.tcp_port
        );
        if let Err(e) = self.apply_inner() {
            warn!(
                "egress: setup failed for {}, tearing down: {e:#}",
                self.netns
            );
            self.teardown();
            return Err(e);
        }
        Ok(())
    }

    fn apply_inner(&self) -> Result<()> {
        for argv in self.setup_commands() {
            run(&argv, None)?;
        }
        let ruleset = self.nft_ruleset();
        run(&self.nft_command(), Some(ruleset.as_bytes()))?;
        debug!("egress: namespace {} ready", self.netns);
        Ok(())
    }

    /// Remove the namespace and veth. Best-effort: logs but does not fail, so it
    /// is safe to call on a cleanup path.
    pub fn teardown(&self) {
        if !privileged() {
            warn!("egress teardown skipped: not privileged");
            return;
        }
        for argv in self.teardown_commands() {
            if let Err(e) = run(&argv, None) {
                debug!("egress: teardown step {:?} failed (ignored): {e:#}", argv);
            }
        }
    }
}

/// True when the process can manage namespaces/links. We approximate with
/// effective uid 0, which is how the runner spawns these operations.
fn privileged() -> bool {
    // SAFETY: geteuid is always safe.
    unsafe { libc::geteuid() == 0 }
}

/// Run an argv, optionally feeding `stdin`, surfacing stderr on failure.
fn run(argv: &[String], stdin: Option<&[u8]>) -> Result<()> {
    let (cmd, rest) = argv.split_first().context("empty command in egress plan")?;
    let mut command = Command::new(cmd);
    command.args(rest);
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    command.stdout(Stdio::null()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("spawning `{}`", argv.join(" ")))?;
    if let Some(data) = stdin {
        child
            .stdin
            .take()
            .context("child stdin unavailable")?
            .write_all(data)
            .context("writing ruleset to nft stdin")?;
    }
    let out = child
        .wait_with_output()
        .with_context(|| format!("waiting on `{}`", argv.join(" ")))?;
    if !out.status.success() {
        bail!(
            "`{}` failed ({}): {}",
            argv.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy() -> EgressProxy {
        EgressProxy {
            tcp_port: 8443,
            dns_port: 5353,
        }
    }

    #[test]
    fn subnet_indices_are_deterministic_and_distinct() {
        let (h0, g0) = subnet_for_index(0);
        assert_eq!(h0, Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(g0, Ipv4Addr::new(10, 0, 0, 2));

        let (h1, g1) = subnet_for_index(1);
        assert_eq!(h1, Ipv4Addr::new(10, 0, 0, 5));
        assert_eq!(g1, Ipv4Addr::new(10, 0, 0, 6));

        // No two indices share an address.
        let mut seen = std::collections::HashSet::new();
        for i in 0..2000u32 {
            let (h, g) = subnet_for_index(i);
            assert!(seen.insert(h), "duplicate host ip at {i}");
            assert!(seen.insert(g), "duplicate guest ip at {i}");
        }
    }

    #[test]
    fn high_index_stays_in_10_8() {
        let (h, g) = subnet_for_index(MAX_INDEX);
        assert_eq!(h.octets()[0], 10);
        assert_eq!(g.octets()[0], 10);
    }

    #[test]
    fn new_rejects_out_of_range_index() {
        assert!(EgressPlan::new(MAX_INDEX + 1, proxy(), vec![]).is_err());
    }

    #[test]
    fn new_rejects_zero_ports() {
        let bad = EgressProxy {
            tcp_port: 0,
            dns_port: 53,
        };
        assert!(EgressPlan::new(0, bad, vec![]).is_err());
    }

    #[test]
    fn interface_names_fit_ifnamsiz() {
        // IFNAMSIZ is 16 incl. NUL, so the usable name is <= 15 chars.
        let plan = EgressPlan::new(MAX_INDEX, proxy(), vec![]).unwrap();
        assert!(plan.host_veth.len() <= 15, "{}", plan.host_veth);
        assert!(plan.guest_veth.len() <= 15, "{}", plan.guest_veth);
    }

    #[test]
    fn from_config_open_and_isolated_yield_no_plan() {
        for mode in [NetworkMode::Open, NetworkMode::Isolated] {
            let cfg = NetworkConfig {
                mode,
                ..Default::default()
            };
            assert!(EgressPlan::from_network_config(&cfg, 0).unwrap().is_none());
        }
    }

    #[test]
    fn from_config_proxy_without_port_fails_closed() {
        let cfg = NetworkConfig {
            mode: NetworkMode::Proxy,
            proxy_tcp_port: None,
            ..Default::default()
        };
        let err = EgressPlan::from_network_config(&cfg, 0).unwrap_err();
        assert!(format!("{err:#}").contains("fail-closed"));
    }

    #[test]
    fn from_config_proxy_defaults_dns_port_to_tcp_port() {
        let cfg = NetworkConfig {
            mode: NetworkMode::Proxy,
            proxy_tcp_port: Some(9000),
            proxy_dns_port: None,
            allow_domains: vec!["api.github.com".into()],
            ..Default::default()
        };
        let plan = EgressPlan::from_network_config(&cfg, 3)
            .unwrap()
            .expect("proxy mode yields a plan");
        assert_eq!(plan.proxy.tcp_port, 9000);
        assert_eq!(plan.proxy.dns_port, 9000);
        assert_eq!(plan.allow_domains, vec!["api.github.com".to_string()]);
    }

    #[test]
    fn setup_commands_have_expected_shape() {
        let plan = EgressPlan::new(7, proxy(), vec![]).unwrap();
        let cmds = plan.setup_commands();
        // First creates the namespace; one step moves the guest veth into it.
        assert_eq!(cmds[0], vec!["ip", "netns", "add", "oqto-egr-7"]);
        assert!(
            cmds.iter()
                .any(|c| c.contains(&"netns".to_string()) && c.contains(&plan.guest_veth)),
            "guest veth must be moved into the namespace"
        );
        // Default route points at the host veth IP (the only reachable peer).
        assert!(cmds.iter().any(|c| {
            c.windows(2)
                .any(|w| w == ["via", plan.host_ip.to_string().as_str()])
        }));
    }

    #[test]
    fn nft_ruleset_captures_and_drops() {
        let plan = EgressPlan::new(0, proxy(), vec![]).unwrap();
        let rs = plan.nft_ruleset();
        // Default-drop on the filter output chain.
        assert!(rs.contains("policy drop"), "must default-drop:\n{rs}");
        // All other TCP is redirected to the transparent proxy.
        assert!(rs.contains("meta l4proto tcp dnat ip to 10.0.0.1:8443"));
        // DNS is forced to the resolver.
        assert!(rs.contains("udp dport 53 dnat ip to 10.0.0.1:5353"));
        assert!(rs.contains("tcp dport 53 dnat ip to 10.0.0.1:5353"));
        // Redirected traffic (now to the host veth) is permitted.
        assert!(rs.contains("ip daddr 10.0.0.1 accept"));
        // No NAT back out to the internet: capture must not become a gateway.
        assert!(!rs.contains("masquerade"), "must not masquerade:\n{rs}");
        assert!(!rs.contains("snat"), "must not snat:\n{rs}");
    }

    #[test]
    fn nft_command_runs_inside_namespace_from_stdin() {
        let plan = EgressPlan::new(2, proxy(), vec![]).unwrap();
        assert_eq!(
            plan.nft_command(),
            vec!["ip", "netns", "exec", "oqto-egr-2", "nft", "-f", "-"]
        );
    }

    #[test]
    fn teardown_removes_namespace_and_host_veth() {
        let plan = EgressPlan::new(5, proxy(), vec![]).unwrap();
        let cmds = plan.teardown_commands();
        assert_eq!(cmds[0], vec!["ip", "netns", "del", "oqto-egr-5"]);
        assert!(cmds.iter().any(|c| c
            == &vec![
                "ip".to_string(),
                "link".to_string(),
                "del".to_string(),
                plan.host_veth.clone()
            ]));
    }

    #[test]
    fn wrap_command_prefixes_netns_exec_without_unshare_net() {
        let plan = EgressPlan::new(1, proxy(), vec![]).unwrap();
        let inner = vec![
            "bwrap".to_string(),
            "--ro-bind".to_string(),
            "/usr".to_string(),
            "/usr".to_string(),
            "--".to_string(),
            "pi".to_string(),
        ];
        let wrapped = plan.wrap_command(&inner);
        assert_eq!(&wrapped[..4], &["ip", "netns", "exec", "oqto-egr-1"]);
        assert_eq!(&wrapped[4..], &inner[..]);
        assert!(
            !wrapped.iter().any(|a| a == "--unshare-net"),
            "proxy mode must not unshare-net"
        );
    }

    /// Live lifecycle against the real kernel. Requires CAP_NET_ADMIN and `nft`,
    /// so it is `#[ignore]`d: run on a privileged host with
    /// `cargo test -p oqto-sandbox -- --ignored egress_live`.
    #[test]
    #[ignore = "requires CAP_NET_ADMIN + nft; run on a privileged host"]
    fn egress_live_namespace_roundtrip() {
        let plan = EgressPlan::new(4_000_001, proxy(), vec![]).unwrap();
        plan.teardown(); // clean any stale state from a previous run
        plan.apply().expect("apply egress namespace");

        // The namespace exists and the redirect ruleset is installed.
        let listed = Command::new("ip").args(["netns", "list"]).output().unwrap();
        assert!(String::from_utf8_lossy(&listed.stdout).contains(&plan.netns));

        let ruleset = Command::new("ip")
            .args(["netns", "exec", &plan.netns, "nft", "list", "ruleset"])
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&ruleset.stdout).contains("oqto_egress"));

        plan.teardown();
        let after = Command::new("ip").args(["netns", "list"]).output().unwrap();
        assert!(!String::from_utf8_lossy(&after.stdout).contains(&plan.netns));
    }
}
