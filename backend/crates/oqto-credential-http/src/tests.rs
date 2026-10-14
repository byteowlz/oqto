use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn paths_and_addresses() {
    for path in [
        "https://evil/a",
        "//evil",
        "../x",
        "a/./x",
        "%2e%2e/x",
        "a%2fx",
        "%5c",
        "%252e",
        "x\r\n",
        "x?authorization=Bearer:x",
        "x#frag",
    ] {
        assert_eq!(validate_relative(path), Err(Error::Input), "{path}");
    }
    assert!(validate_relative("users/list?q=hello%20world").is_err());
    assert!(validate_relative("users/list?q=hello").is_ok());
    for address in [
        "127.0.0.1",
        "10.1.2.3",
        "100.64.0.1",
        "169.254.169.254",
        "168.63.129.16",
        "192.0.0.9",
        "198.18.0.1",
        "224.1.1.1",
        "240.0.0.1",
        "::1",
        "::ffff:8.8.8.8",
        "2002:808:808::1",
        "2001:db8::1",
        "fc00::1",
        "ff02::1",
    ] {
        assert!(!public_ip(address.parse().unwrap()), "{address}");
    }
    assert!(public_ip("8.8.8.8".parse().unwrap()));
    assert!(public_ip("2606:4700::1111".parse().unwrap()));
}

#[test]
fn policy_and_echo() {
    for host in [
        "localhost",
        "127.0.0.1",
        "good.com:443",
        "user@good.com",
        "GOOD.com",
        "good.com.",
    ] {
        assert!(
            DestinationPolicy::new(
                host,
                "/v1/",
                CredentialHeader::Authorization,
                Attachment::Bearer,
                1
            )
            .is_err()
        );
    }
    for prefix in ["/v1", "/../", "/%2e/", "//evil/", "/v1/?x/"] {
        assert!(
            DestinationPolicy::new(
                "api.example.com",
                prefix,
                CredentialHeader::Authorization,
                Attachment::Bearer,
                1
            )
            .is_err()
        );
    }
    let v: serde_json::Value =
        serde_json::from_str(r#"{"nested":["synthetic\u002dtoken"]}"#).unwrap();
    assert!(echoed(&v, "synthetic-token"));
    assert_eq!(
        format!("{:?}", JsonResult { json: v, bytes: 1 }),
        "JsonResult([REDACTED])"
    );
    assert!(!format!("{:?} {}", Error::Upstream, Error::Upstream).contains("synthetic-token"));
}

struct Deny;
impl TrustedAuthority for Deny {
    fn audit_before_act(&self, _: &UseEvidence) -> Result<(), Error> {
        Err(Error::Audit)
    }
    fn resolve_for_use(&self, _: &UseEvidence) -> Result<SecretValue, Error> {
        panic!("audit must precede resolution")
    }
}
#[tokio::test]
async fn audit_fails_before_dns_or_resolution() {
    let policy = DestinationPolicy::new(
        "never-resolve.invalid",
        "/v1/",
        CredentialHeader::Authorization,
        Attachment::Bearer,
        2,
    )
    .unwrap();
    assert!(
        AuthorizedUse::new(
            DestinationPolicy::new(
                "api.example.com",
                "/",
                CredentialHeader::Authorization,
                Attachment::Raw,
                2
            )
            .unwrap(),
            UseEvidence {
                subject: "s".into(),
                authorization_id: "a".into(),
                policy_revision: 1
            }
        )
        .is_err()
    );
    let lease = AuthorizedUse::new(
        policy,
        UseEvidence {
            subject: "s".into(),
            authorization_id: "a".into(),
            policy_revision: 2,
        },
    )
    .unwrap();
    assert_eq!(
        get_json(&Deny, lease, "ok").await.unwrap_err(),
        Error::Audit
    );
}

// Test-only TLS transport below production DNS/policy. No production override exists.
async fn fixture(status: &str, body: &str, content_type: &str) -> reqwest::Response {
    let certified = rcgen::generate_simple_self_signed(vec!["api.example.com".into()]).unwrap();
    let cert = certified.cert.der().clone();
    let key = tokio_rustls::rustls::pki_types::PrivatePkcs8KeyDer::from(
        certified.signing_key.serialize_der(),
    );
    let config = tokio_rustls::rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key.into())
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let reply = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nLocation: https://evil.example/\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut tls = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(config))
            .accept(socket)
            .await
            .unwrap();
        let mut request = [0; 4096];
        let _ = tls.read(&mut request).await.unwrap();
        tls.write_all(reply.as_bytes()).await.unwrap();
    });
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .add_root_certificate(reqwest::Certificate::from_der(&cert).unwrap())
        .resolve("api.example.com", addr)
        .build()
        .unwrap()
        .get(format!("https://api.example.com:{}/v1/ok", addr.port()))
        .send()
        .await
        .unwrap()
}
#[tokio::test]
async fn real_tls_bounded_json() {
    let response = fixture("200 OK", r#"{"ok":true}"#, "application/json").await;
    assert_eq!(
        read_json(response, "synthetic-token").await.unwrap().json["ok"],
        true
    );
    for (status, body, mime) in [
        ("302 Found", "{}".into(), "application/json"),
        ("500 Failure", "synthetic-token".into(), "application/json"),
        (
            "200 OK",
            r#"{"echo":"synthetic\u002dtoken"}"#.into(),
            "application/json",
        ),
        (
            "200 OK",
            r#"{"echo":"synthetic-token"}"#.into(),
            "application/json",
        ),
        ("200 OK", "x".repeat(CAP + 1), "application/json"),
        ("200 OK", "{}".into(), "text/plain"),
        ("200 OK", "not-json".into(), "application/json"),
    ] {
        assert_eq!(
            read_json(fixture(status, &body, mime).await, "synthetic-token")
                .await
                .unwrap_err(),
            Error::Output
        );
    }
}
