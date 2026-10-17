//! Host-neutral personal pairing wire; no SQLite, service, Account or session supervisor.
//! QR output contains a one-use secret. Never send it to chat, analytics or logs.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;
pub use xlatch_wire_auth::{Enrollment, OQTO_PERSONAL_V1, SignedRequest};

#[cfg(feature = "tls")]
pub mod tls;

pub const ENROLL_PATH: &str = "/personal/v1/enroll";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentReply {
    pub version: u8,
    pub host_id: String,
    pub device_id: String,
}

pub const PURPOSE: &str = "oqto.personal-host.v1";
pub const MAX_QR_BYTES: usize = 2048;
pub const MAX_ENROLLMENT_BYTES: usize = 4096;
pub const PAIRING_TTL_SECONDS: i64 = 300;
pub const QR_CLOCK_SKEW_SECONDS: i64 = 60;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QrPayload {
    version: u8,
    purpose: String,
    host_id: String,
    url: String,
    pin: String,
    token: String,
    expires_at: i64,
}

/// Validated purpose-specific QR. Intentionally not Debug or Deserialize;
/// clients must use bounded, clock-aware `parse`, not unchecked field decoding.
#[derive(Serialize)]
#[serde(transparent)]
pub struct PairingQr(QrPayload);

impl PairingQr {
    /// Build from the actual HTTPS server's configured leaf DER certificate.
    /// Certificate parsing/validity and listener reachability remain transport responsibilities.
    pub fn new(
        host_id: &str,
        origin: &str,
        leaf_der: &[u8],
        token: String,
        expires_at: i64,
        now: i64,
    ) -> Result<Self> {
        ensure!(
            !leaf_der.is_empty() && leaf_der.len() <= 65536,
            "invalid server certificate bounds"
        );
        let result = Self(QrPayload {
            version: 1,
            purpose: PURPOSE.into(),
            host_id: host_id.into(),
            url: validate_origin(origin)?,
            pin: hex::encode(Sha256::digest(leaf_der)),
            token,
            expires_at,
        });
        result.validate(now, PAIRING_TTL_SECONDS)?;
        ensure!(
            result.to_json()?.len() <= MAX_QR_BYTES,
            "pairing QR too large"
        );
        Ok(result)
    }

    pub fn parse(bytes: &[u8], now: i64) -> Result<Self> {
        ensure!(bytes.len() <= MAX_QR_BYTES, "pairing QR too large");
        let payload = serde_json::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("invalid Oqto personal pairing QR"))?;
        let result = Self(payload);
        result.validate(now, PAIRING_TTL_SECONDS + QR_CLOCK_SKEW_SECONDS)?;
        Ok(result)
    }

    fn validate(&self, now: i64, max_future: i64) -> Result<()> {
        let payload = &self.0;
        ensure!(
            payload.version == 1 && payload.purpose == PURPOSE,
            "unsupported pairing purpose or version"
        );
        ensure!(
            Uuid::parse_str(&payload.host_id).is_ok_and(|id| id.to_string() == payload.host_id),
            "invalid host identity"
        );
        ensure!(
            validate_origin(&payload.url)? == payload.url,
            "pairing origin must be canonical"
        );
        ensure!(
            hex64(&payload.pin) && hex64(&payload.token),
            "invalid pairing pin or token"
        );
        ensure!(
            now >= 0
                && payload.expires_at > now
                && payload
                    .expires_at
                    .checked_sub(now)
                    .is_some_and(|remaining| remaining <= max_future),
            "pairing QR expired or invalid clock"
        );
        Ok(())
    }

    pub fn host_id(&self) -> &str {
        &self.0.host_id
    }
    pub fn origin(&self) -> &str {
        &self.0.url
    }
    pub fn certificate_sha256(&self) -> &str {
        &self.0.pin
    }
    pub fn expires_at(&self) -> i64 {
        self.0.expires_at
    }
    /// Sensitive: use only to construct the signed enrollment proof over pinned TLS.
    pub fn enrollment_token(&self) -> &str {
        &self.0.token
    }
    /// Sensitive: explicit QR/CLI output only; never a diagnostic receipt.
    pub fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
    /// The peer must match this exact leaf fingerprint. This is not a substitute
    /// for TLS handshake-signature verification or application grant checks.
    pub fn matches_leaf(&self, leaf_der: &[u8]) -> bool {
        hex::encode(Sha256::digest(leaf_der)) == self.0.pin
    }

    #[cfg(feature = "qr")]
    pub fn render_terminal(&self) -> Result<String> {
        let code = qrcode::QrCode::new(self.to_json()?)?;
        Ok(code.render::<qrcode::render::unicode::Dense1x2>().build())
    }
    #[cfg(feature = "qr")]
    pub fn render_svg(&self) -> Result<String> {
        let code = qrcode::QrCode::new(self.to_json()?)?;
        Ok(code.render::<qrcode::render::svg::Color<'_>>().build())
    }
}

/// Accept only a root HTTPS origin. Never silently discard credentials, query
/// tokens, fragments or an application path supplied in a QR.
pub fn validate_origin(origin: &str) -> Result<String> {
    ensure!(
        origin.len() <= 1024
            && !origin.chars().any(char::is_whitespace)
            && !origin.chars().any(char::is_control),
        "invalid pairing origin bounds"
    );
    let url = Url::parse(origin).map_err(|_| anyhow::anyhow!("invalid pairing HTTPS origin"))?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "pairing requires a root HTTPS origin without credentials, query or fragment"
    );
    let canonical = url.to_string();
    ensure!(
        canonical.len() <= 1024,
        "canonical pairing origin too large"
    );
    Ok(canonical)
}

/// Bound untrusted enrollment input before allocating/validating its fields.
/// This does NOT verify proof, consume tickets or grant capabilities.
pub fn decode_enrollment(bytes: &[u8]) -> Result<Enrollment> {
    ensure!(
        bytes.len() <= MAX_ENROLLMENT_BYTES,
        "enrollment request too large"
    );
    serde_json::from_slice(bytes).map_err(|_| anyhow::anyhow!("invalid enrollment request"))
}

fn hex64(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests;
