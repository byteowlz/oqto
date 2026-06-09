//! Emit the exact `nft` ruleset and `ip` setup commands an [`EgressPlan`]
//! generates, so they can be validated against a live kernel (e.g. inside an
//! unprivileged user+net namespace). Pure; needs no privileges to run.
//!
//! `cargo run -p oqto-sandbox --example egress_emit -- ruleset`
//! `cargo run -p oqto-sandbox --example egress_emit -- setup`

use oqto_sandbox::{EgressPlan, EgressProxy};

fn main() {
    let plan = EgressPlan::new(
        7,
        EgressProxy {
            tcp_port: 8443,
            dns_port: 5353,
        },
        vec!["api.github.com".to_string()],
    )
    .expect("plan");

    match std::env::args().nth(1).as_deref() {
        Some("ruleset") => print!("{}", plan.nft_ruleset()),
        Some("setup") => {
            for c in plan.setup_commands() {
                println!("{}", c.join(" "));
            }
        }
        _ => {
            eprintln!("usage: egress_emit <ruleset|setup>");
            std::process::exit(2);
        }
    }
}
