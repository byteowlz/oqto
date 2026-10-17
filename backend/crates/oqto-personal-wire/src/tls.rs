//! Exact-leaf trust for owner-pinned personal hosts. This intentionally uses
//! the QR pin, not public PKIX CA/name/time trust. TLS CertificateVerify remains
//! mandatory: matching a public certificate without its private key is not enough.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio_rustls::rustls::{
    self, ClientConfig, DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::CryptoProvider,
    pki_types::{CertificateDer, ServerName, UnixTime},
};

#[derive(Debug)]
struct PinnedLeaf {
    fingerprint: String,
    provider: Arc<CryptoProvider>,
}
impl ServerCertVerifier for PinnedLeaf {
    fn verify_server_cert(
        &self,
        certificate: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        if hex::encode(Sha256::digest(certificate.as_ref())) != self.fingerprint {
            return Err(rustls::Error::General(
                "paired host certificate pin mismatch".into(),
            ));
        }
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Use the stored owner-approved leaf pin; no CA fallback, accept-any mode or
/// TLS early data. Callers must connect only to their validated pairing origin.
/// Explicit pin trust does not assert PKIX certificate lifetime or DNS validation.
pub fn pinned_client_config(fingerprint: &str) -> Result<ClientConfig> {
    ensure!(
        crate::hex64(fingerprint),
        "invalid paired host certificate pin"
    );
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = PinnedLeaf {
        fingerprint: fingerprint.into(),
        provider: Arc::clone(&provider),
    };
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    config.enable_early_data = false;
    // Every connection must present and prove the exact pinned certificate.
    config.resumption = rustls::client::Resumption::disabled();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}
