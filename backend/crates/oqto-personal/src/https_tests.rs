use super::*;
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tokio_rustls::{
    TlsConnector,
    rustls::{
        self, ServerConfig,
        pki_types::{PrivatePkcs8KeyDer, ServerName},
        sign::{CertifiedKey, Signer, SigningKey, SingleCertAndKey},
    },
};

#[derive(Debug)]
struct CopiedCertificateWithoutKey;
impl SigningKey for CopiedCertificateWithoutKey {
    fn choose_scheme(&self, offered: &[rustls::SignatureScheme]) -> Option<Box<dyn Signer>> {
        offered
            .contains(&rustls::SignatureScheme::ECDSA_NISTP256_SHA256)
            .then(|| Box::new(Self) as Box<dyn Signer>)
    }
    fn algorithm(&self) -> rustls::SignatureAlgorithm {
        rustls::SignatureAlgorithm::ECDSA
    }
}
impl Signer for CopiedCertificateWithoutKey {
    fn sign(&self, _message: &[u8]) -> std::result::Result<Vec<u8>, rustls::Error> {
        Ok(vec![0; 64])
    }
    fn scheme(&self) -> rustls::SignatureScheme {
        rustls::SignatureScheme::ECDSA_NISTP256_SHA256
    }
}
fn identity(copied_without_key: bool) -> Result<(Arc<ServerConfig>, Vec<u8>)> {
    let generated = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    let leaf = generated.cert.der().clone();
    let bytes = leaf.to_vec();
    let builder =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_no_client_auth();
    let config = if copied_without_key {
        let key = CertifiedKey::new(vec![leaf], Arc::new(CopiedCertificateWithoutKey));
        builder.with_cert_resolver(Arc::new(SingleCertAndKey::from(key)))
    } else {
        builder.with_single_cert(
            vec![leaf],
            PrivatePkcs8KeyDer::from(generated.key_pair.serialize_der()).into(),
        )?
    };
    Ok((Arc::new(config), bytes))
}
async fn start(
    authority: Arc<PairingAuthority>,
    config: Arc<ServerConfig>,
) -> Result<(
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<()>>,
)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = oneshot::channel();
    let task = tokio::spawn(http::serve_enrollment_tls(
        listener,
        config,
        authority,
        async {
            let _ = stopped.await;
        },
    ));
    Ok((address, stop, task))
}
async fn post(address: SocketAddr, pin: &str, body: &[u8]) -> Result<String> {
    let connector = TlsConnector::from(Arc::new(oqto_personal_wire::tls::pinned_client_config(
        pin,
    )?));
    let socket = TcpStream::connect(address).await?;
    // No request bytes (including the ticket) are written before successful TLS.
    let mut stream = connector
        .connect(ServerName::try_from("localhost")?.to_owned(), socket)
        .await?;
    let header = format!(
        "POST {} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        oqto_personal_wire::ENROLL_PATH,
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response)).await??;
    Ok(String::from_utf8(response)?)
}

#[tokio::test]
async fn real_pinned_https_enrollment_rejects_replay_widening_and_oversize() -> Result<()> {
    let (_dir, authority) = tests::fixture().await?;
    let authority = Arc::new(authority);
    let (config, leaf) = identity(false)?;
    let (address, stop, task) = start(Arc::clone(&authority), config).await?;
    let qr = authority
        .issue_pairing_qr(&Grants::default(), &format!("https://{address}"), &leaf)
        .await?;
    let ticket = PairingTicket {
        token: qr.enrollment_token().into(),
        expires_at: qr.expires_at(),
    };
    let proof = tests::enrollment(&ticket, 1);
    let mut widened = serde_json::to_value(&proof)?;
    widened["grants"] = serde_json::json!({"full_user_terminal":true});
    assert!(
        post(
            address,
            qr.certificate_sha256(),
            &serde_json::to_vec(&widened)?
        )
        .await?
        .starts_with("HTTP/1.1 401")
    );
    assert!(
        post(address, qr.certificate_sha256(), &vec![b' '; 4097])
            .await?
            .starts_with("HTTP/1.1 413")
    );
    let body = serde_json::to_vec(&proof)?;
    let response = post(address, qr.certificate_sha256(), &body).await?;
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains(authority.host_id()));
    assert!(!response.contains(qr.enrollment_token()));
    assert!(
        post(address, qr.certificate_sha256(), &body)
            .await?
            .starts_with("HTTP/1.1 401")
    );
    let _ = stop.send(());
    task.await??;
    Ok(())
}

#[tokio::test]
async fn expired_ticket_is_refused_over_real_tls() -> Result<()> {
    let (_dir, authority) = tests::fixture().await?;
    let authority = Arc::new(authority);
    let (config, leaf) = identity(false)?;
    let (address, stop, task) = start(Arc::clone(&authority), config).await?;
    let qr = authority
        .issue_pairing_qr(&Grants::default(), &format!("https://{address}"), &leaf)
        .await?;
    let ticket = PairingTicket {
        token: qr.enrollment_token().into(),
        expires_at: qr.expires_at(),
    };
    sqlx::query("UPDATE personal_pairing_tickets SET expires_at=0")
        .execute(&authority.pool)
        .await?;
    let response = post(
        address,
        qr.certificate_sha256(),
        &serde_json::to_vec(&tests::enrollment(&ticket, 1))?,
    )
    .await?;
    assert!(response.starts_with("HTTP/1.1 401"));
    assert!(!response.contains(qr.enrollment_token()));
    let _ = stop.send(());
    task.await??;
    Ok(())
}

#[tokio::test]
async fn wrong_pin_or_copied_certificate_cannot_admit_ticket() -> Result<()> {
    for copied in [false, true] {
        let (_dir, authority) = tests::fixture().await?;
        let authority = Arc::new(authority);
        let (config, leaf) = identity(copied)?;
        let (address, stop, task) = start(Arc::clone(&authority), config).await?;
        let qr = authority
            .issue_pairing_qr(&Grants::default(), &format!("https://{address}"), &leaf)
            .await?;
        let ticket = PairingTicket {
            token: qr.enrollment_token().into(),
            expires_at: qr.expires_at(),
        };
        let body = serde_json::to_vec(&tests::enrollment(&ticket, 1))?;
        let pin = if copied {
            qr.certificate_sha256().to_owned()
        } else {
            "0".repeat(64)
        };
        assert!(post(address, &pin, &body).await.is_err());
        let devices: i64 = sqlx::query_scalar("SELECT count(*) FROM personal_devices")
            .fetch_one(&authority.pool)
            .await?;
        assert_eq!(devices, 0);
        let consumed: Option<i64> =
            sqlx::query_scalar("SELECT consumed_at FROM personal_pairing_tickets")
                .fetch_one(&authority.pool)
                .await?;
        assert!(consumed.is_none());
        let _ = stop.send(());
        task.await??;
    }
    Ok(())
}
