//! Existing trusted-local admin policy. Peer evidence is never replaced with
//! the server's own uid, including on platforms where evidence is unavailable.
use std::io;

pub fn authorize_peer_uid(peer_uid: io::Result<u32>, server_uid: u32) -> io::Result<()> {
    let uid = peer_uid?;
    if uid == 0 || uid == server_uid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "admin socket peer uid denied",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_and_error_evidence_fail_closed() {
        for kind in [
            io::ErrorKind::Unsupported,
            io::ErrorKind::NotConnected,
            io::ErrorKind::Other,
        ] {
            let error =
                authorize_peer_uid(Err(io::Error::new(kind, "synthetic peer failure")), 501)
                    .unwrap_err();
            assert_eq!(error.kind(), kind);
        }
    }

    #[test]
    fn different_uid_never_becomes_server_uid() {
        assert_eq!(
            authorize_peer_uid(Ok(502), 501).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(authorize_peer_uid(Ok(501), 501).is_ok());
        assert!(authorize_peer_uid(Ok(0), 501).is_ok());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn real_same_uid_peer_primitive() -> io::Result<()> {
        let (socket, _peer) = tokio::net::UnixStream::pair()?;
        let uid = socket.peer_cred()?.uid();
        // Read-only probe. Different-UID/root platform probes require a separately
        // authorized lane; policy denial above uses synthetic credential evidence.
        assert_eq!(uid, unsafe { libc::geteuid() });
        authorize_peer_uid(socket.peer_cred().map(|cred| cred.uid()), uid)
    }
}
