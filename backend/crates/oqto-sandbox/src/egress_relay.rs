//! Support for `oqto-egress-relay`, the in-namespace forwarder that conveys an
//! agent's real destination to eavs.
//!
//! In `NetworkMode::Proxy` the agent's TCP egress is DNAT'd (inside its own
//! network namespace) to the relay, which runs in that same namespace. Because
//! the DNAT and its conntrack record live in the namespace, the relay can read
//! the original destination via `SO_ORIGINAL_DST` -- something a host-side proxy
//! cannot do across the veth. The relay then connects to eavs and prepends a
//! **PROXY protocol v2** header carrying that destination before splicing the
//! bytes through.
//!
//! This module holds the pieces shared between the library (spawn/teardown) and
//! the `oqto-egress-relay` binary: the PROXY v2 header encoder and the binary
//! path resolver (mirroring [`crate::landlock_shim::resolve_shim_binary`]).
//!
//! The relay is pure mechanism: it carries no ACL or secrets. Policy
//! (allow/deny, credential injection) lives entirely in eavs.

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

/// Env var overriding the resolved `oqto-egress-relay` binary path (tests/deploy).
pub const RELAY_BIN_OVERRIDE_ENV: &str = "OQTO_EGRESS_RELAY_BIN";

/// Env var the relay reads for the eavs transparent endpoint (`ip:port`).
pub const RELAY_EAVS_ENV: &str = "OQTO_EGRESS_RELAY_EAVS";

/// Env var the relay reads for its in-namespace listen address (`ip:port`).
pub const RELAY_LISTEN_ENV: &str = "OQTO_EGRESS_RELAY_LISTEN";

/// 12-byte PROXY protocol v2 signature.
pub const PROXY_V2_SIG: [u8; 12] = [
    0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A,
];

/// Encode a PROXY protocol v2 header announcing `src -> dst` for a STREAM
/// connection. `src` and `dst` must share an address family; the family of
/// `dst` is authoritative (mismatched `src` is replaced with the unspecified
/// address of `dst`'s family).
pub fn proxy_v2_header(src: SocketAddr, dst: SocketAddr) -> Vec<u8> {
    let mut out = Vec::with_capacity(28);
    out.extend_from_slice(&PROXY_V2_SIG);
    out.push(0x21); // version 2, command PROXY

    match dst {
        SocketAddr::V4(d) => {
            out.push(0x11); // AF_INET, STREAM
            out.extend_from_slice(&12u16.to_be_bytes());
            let s = match src {
                SocketAddr::V4(s) => *s.ip(),
                SocketAddr::V6(_) => std::net::Ipv4Addr::UNSPECIFIED,
            };
            out.extend_from_slice(&s.octets());
            out.extend_from_slice(&d.ip().octets());
            let sport = if src.is_ipv4() { src.port() } else { 0 };
            out.extend_from_slice(&sport.to_be_bytes());
            out.extend_from_slice(&d.port().to_be_bytes());
        }
        SocketAddr::V6(d) => {
            out.push(0x21); // AF_INET6, STREAM
            out.extend_from_slice(&36u16.to_be_bytes());
            let s = match src {
                SocketAddr::V6(s) => *s.ip(),
                SocketAddr::V4(_) => std::net::Ipv6Addr::UNSPECIFIED,
            };
            out.extend_from_slice(&s.octets());
            out.extend_from_slice(&d.ip().octets());
            let sport = if src.is_ipv6() { src.port() } else { 0 };
            out.extend_from_slice(&sport.to_be_bytes());
            out.extend_from_slice(&d.port().to_be_bytes());
        }
    }
    out
}

/// Resolve the `oqto-egress-relay` binary: explicit override, then next to the
/// current executable, then `PATH`. Mirrors the landlock shim resolver so the
/// relay is deployed and located the same way.
pub fn resolve_relay_binary() -> Option<PathBuf> {
    if let Ok(raw) = env::var(RELAY_BIN_OVERRIDE_ENV)
        && !raw.is_empty()
    {
        let p = PathBuf::from(raw);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join("oqto-egress-relay");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    which_on_path("oqto-egress-relay")
}

/// Find a binary on `PATH` (small dependency-free lookup).
fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn proxy_v2_header_tcp4_matches_wire_format() {
        let src = SocketAddr::from((Ipv4Addr::new(10, 0, 0, 2), 54321));
        let dst = SocketAddr::from((Ipv4Addr::new(140, 82, 112, 3), 443));
        let h = proxy_v2_header(src, dst);

        assert_eq!(&h[..12], &PROXY_V2_SIG);
        assert_eq!(h[12], 0x21); // v2 PROXY
        assert_eq!(h[13], 0x11); // AF_INET STREAM
        assert_eq!(u16::from_be_bytes([h[14], h[15]]), 12);
        assert_eq!(&h[16..20], &[10, 0, 0, 2]); // src
        assert_eq!(&h[20..24], &[140, 82, 112, 3]); // dst
        assert_eq!(u16::from_be_bytes([h[24], h[25]]), 54321); // sport
        assert_eq!(u16::from_be_bytes([h[26], h[27]]), 443); // dport
        assert_eq!(h.len(), 28);
    }

    #[test]
    fn proxy_v2_header_tcp6_has_correct_length_and_family() {
        let src = SocketAddr::from((Ipv6Addr::LOCALHOST, 1234));
        let dst = SocketAddr::from((Ipv6Addr::new(0x2606, 0x4700, 0, 0, 0, 0, 0, 1), 443));
        let h = proxy_v2_header(src, dst);
        assert_eq!(h[13], 0x21); // AF_INET6 STREAM
        assert_eq!(u16::from_be_bytes([h[14], h[15]]), 36);
        assert_eq!(h.len(), 16 + 36);
        // dst address occupies bytes 16+16 .. 16+32
        assert_eq!(
            &h[32..48],
            &dst.ip().to_string().parse::<Ipv6Addr>().unwrap().octets()
        );
        assert_eq!(u16::from_be_bytes([h[50], h[51]]), 443);
    }

    #[test]
    fn resolve_relay_binary_honors_override() {
        // Point at a path that exists (this test binary) to exercise the branch.
        let exe = std::env::current_exe().unwrap();
        // SAFETY: single-threaded test; restored after.
        let prev = env::var_os(RELAY_BIN_OVERRIDE_ENV);
        unsafe { env::set_var(RELAY_BIN_OVERRIDE_ENV, &exe) };
        let resolved = resolve_relay_binary();
        match prev {
            Some(v) => unsafe { env::set_var(RELAY_BIN_OVERRIDE_ENV, v) },
            None => unsafe { env::remove_var(RELAY_BIN_OVERRIDE_ENV) },
        }
        assert_eq!(resolved, Some(exe));
    }
}
