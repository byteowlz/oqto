//! Private persistent host transport identity. No user credential files are
//! overwritten, imported or repaired; one SQLite transaction owns both halves.
use crate::*;
use tokio_rustls::rustls::{
    self, ServerConfig,
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
};

pub struct TransportIdentity {
    leaf: Vec<u8>,
    key: Vec<u8>,
}
impl TransportIdentity {
    pub fn leaf_der(&self) -> &[u8] {
        &self.leaf
    }
    pub fn server_config(&self) -> Result<Arc<ServerConfig>> {
        let config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])?
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(self.leaf.clone())],
                    PrivatePkcs8KeyDer::from(self.key.clone()).into(),
                )?;
        Ok(Arc::new(config))
    }
}
impl PairingAuthority {
    pub async fn transport_identity(&self) -> Result<TransportIdentity> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT leaf_der,key_der FROM personal_transport_identity WHERE singleton=1",
        )
        .fetch_optional(&mut *tx)
        .await?;
        let identity = match row {
            Some(row) => TransportIdentity {
                leaf: row.try_get("leaf_der")?,
                key: row.try_get("key_der")?,
            },
            None => {
                let generated =
                    rcgen::generate_simple_self_signed(vec!["oqto-personal-host".into()])?;
                let identity = TransportIdentity {
                    leaf: generated.cert.der().to_vec(),
                    key: generated.key_pair.serialize_der(),
                };
                sqlx::query("INSERT INTO personal_transport_identity(singleton,leaf_der,key_der) VALUES (1,?,?)")
                    .bind(&identity.leaf).bind(&identity.key).execute(&mut *tx).await?;
                identity
            }
        };
        // Invalid stored data is an error, never permission to silently rotate.
        identity.server_config()?;
        tx.commit().await?;
        Ok(identity)
    }
}
