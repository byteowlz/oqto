//! Live validation that the `nft` ruleset emitted by [`EgressPlan`] is accepted
//! by the running kernel.
//!
//! This needs `CAP_NET_ADMIN` over a network namespace. Rather than require
//! root, we obtain it via an unprivileged user+net namespace (`unshare -rn`),
//! which grants namespaced `CAP_NET_ADMIN`. Where that is unavailable (no
//! `unshare`/`nft`, or unprivileged userns disabled), the test skips so it
//! never blocks an unprivileged CI.
//!
//! The named-namespace orchestration in `EgressPlan::apply` (`ip netns add` /
//! `ip netns exec`) still requires real root and is covered by the `#[ignore]`d
//! unit test `egress::tests::egress_live_namespace_roundtrip`.

use std::io::Write;
use std::process::{Command, Stdio};

use oqto_sandbox::{EgressPlan, EgressProxy};

/// True when we can get namespaced CAP_NET_ADMIN and `nft` is present.
fn userns_nft_available() -> bool {
    if Command::new("nft").arg("--version").output().is_err() {
        return false;
    }
    // `unshare -rn true` succeeds only if unprivileged user+net namespaces work.
    matches!(
        Command::new("unshare").args(["-rn", "true"]).status(),
        Ok(s) if s.success()
    )
}

#[test]
fn generated_ruleset_is_accepted_by_live_kernel() {
    if !userns_nft_available() {
        eprintln!("skipping: unprivileged userns + nft not available");
        return;
    }

    let plan = EgressPlan::new(
        7,
        EgressProxy {
            tcp_port: 8443,
            dns_port: 5353,
        },
        vec![],
    )
    .expect("plan");
    let ruleset = plan.nft_ruleset();

    // Load the ruleset inside a throwaway user+net namespace: `nft -f -` reads
    // the ruleset from stdin. Success means the kernel parsed and accepted every
    // rule (dnat targets, hooks, priorities, default-drop policy).
    let mut child = Command::new("unshare")
        .args(["-rn", "nft", "-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn unshare nft");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(ruleset.as_bytes())
        .expect("write ruleset");
    let out = child.wait_with_output().expect("wait");

    assert!(
        out.status.success(),
        "live kernel rejected generated ruleset:\n{}\n--- ruleset ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        ruleset
    );
}
