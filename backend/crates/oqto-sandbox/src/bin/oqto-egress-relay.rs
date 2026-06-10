//! `oqto-egress-relay` -- the in-namespace egress forwarder for
//! `NetworkMode::Proxy`.
//!
//! Runs inside an agent's network namespace (spawned there by the runner). The
//! namespace's nftables rules DNAT the agent's TCP egress to this relay's listen
//! address. For each connection the relay:
//!   1. reads the original destination via `SO_ORIGINAL_DST` (works here because
//!      the DNAT and its conntrack entry are in this same namespace),
//!   2. connects to eavs at the configured transparent endpoint,
//!   3. writes a PROXY protocol v2 header announcing the original destination,
//!   4. splices bytes in both directions.
//!
//! It is pure mechanism: no ACL, no secrets. Policy lives in eavs.
//!
//! Configuration is via env vars set by the spawner:
//!   - `OQTO_EGRESS_RELAY_LISTEN` -- `ip:port` to listen on (in the namespace)
//!   - `OQTO_EGRESS_RELAY_EAVS`   -- `ip:port` of the eavs transparent listener

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::os::fd::{AsRawFd, RawFd};
use std::thread;

use oqto_sandbox::egress_relay::{RELAY_EAVS_ENV, RELAY_LISTEN_ENV, proxy_v2_header};

/// netfilter `SO_ORIGINAL_DST` (not exported by the `libc` crate).
const SO_ORIGINAL_DST: libc::c_int = 80;

fn main() {
    if let Err(e) = run() {
        eprintln!("oqto-egress-relay: fatal: {e}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let listen = env_addr(RELAY_LISTEN_ENV)?;
    let eavs = std::env::var(RELAY_EAVS_ENV)
        .map_err(|_| io::Error::other(format!("{RELAY_EAVS_ENV} not set")))?;

    let listener = TcpListener::bind(listen)?;
    eprintln!("oqto-egress-relay: listening on {listen} -> eavs {eavs}");

    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let eavs = eavs.clone();
                thread::spawn(move || {
                    if let Err(e) = handle(stream, &eavs) {
                        eprintln!("oqto-egress-relay: connection error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("oqto-egress-relay: accept error: {e}"),
        }
    }
    Ok(())
}

fn handle(inbound: TcpStream, eavs: &str) -> io::Result<()> {
    let dst = original_dst(inbound.as_raw_fd())?;
    let src = inbound
        .peer_addr()
        .unwrap_or(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)));

    let mut outbound = TcpStream::connect(eavs)?;
    // Announce the real destination to eavs before any payload.
    let header = proxy_v2_header(src, SocketAddr::V4(dst));
    {
        use std::io::Write;
        outbound.write_all(&header)?;
    }

    splice(inbound, outbound);
    Ok(())
}

/// Bidirectionally copy between two streams until both directions close.
fn splice(a: TcpStream, b: TcpStream) {
    let (mut a_rd, mut b_wr) = (a.try_clone(), b.try_clone());
    let (a2, b2) = (a, b);
    // a -> b
    let t = thread::spawn(move || {
        if let (Ok(ar), Ok(bw)) = (a_rd.as_mut(), b_wr.as_mut()) {
            let _ = io::copy(ar, bw);
            let _ = bw.shutdown(std::net::Shutdown::Write);
        }
    });
    // b -> a (on this thread)
    let mut a_wr = a2;
    let mut b_rd = b2;
    let _ = io::copy(&mut b_rd, &mut a_wr);
    let _ = a_wr.shutdown(std::net::Shutdown::Write);
    let _ = t.join();
}

/// Read the pre-DNAT destination of a connection via `getsockopt(SO_ORIGINAL_DST)`.
/// IPv4 only (the egress subnet pool is IPv4).
fn original_dst(fd: RawFd) -> io::Result<SocketAddrV4> {
    // SAFETY: zeroed sockaddr_in is a valid initial value; getsockopt fills it
    // and writes the used length into `len`.
    let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_IP,
            SO_ORIGINAL_DST,
            &mut addr as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    let ip = Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr));
    let port = u16::from_be(addr.sin_port);
    Ok(SocketAddrV4::new(ip, port))
}

/// Parse an `ip:port` env var into a `SocketAddr`.
fn env_addr(key: &str) -> io::Result<SocketAddr> {
    let raw = std::env::var(key).map_err(|_| io::Error::other(format!("{key} not set")))?;
    raw.parse()
        .map_err(|_| io::Error::other(format!("{key} is not a valid ip:port: {raw}")))
}
