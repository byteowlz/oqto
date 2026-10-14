//! Trusted host-only bounded credential use. This is not a client authorization API.
//! The host must freshly grant use (not mere ownership), bind evidence to the exact
//! credential, and enforce revocation, placement, concurrency and rate limits.
//! Approved upstreams are trusted: transformed secret echoes cannot be removed reliably.
use oqto_secrets::SecretValue;
use reqwest::{Url, header::HeaderValue};
use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    time::Duration,
};
use zeroize::Zeroizing;

const CAP: usize = 256 * 1024;
const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
pub enum CredentialHeader {
    Authorization,
    ReviewedXApiKey,
}
#[derive(Clone, Copy)]
pub enum Attachment {
    Bearer,
    Raw,
}

pub struct DestinationPolicy {
    host: String,
    prefix: String,
    header: CredentialHeader,
    attachment: Attachment,
    revision: u64,
}
impl DestinationPolicy {
    pub fn new(
        host: &str,
        path_prefix: &str,
        header: CredentialHeader,
        attachment: Attachment,
        revision: u64,
    ) -> Result<Self, Error> {
        if host.len() > 253
            || !host.contains('.')
            || host.parse::<IpAddr>().is_ok()
            || host.split('.').any(|s| {
                s.is_empty()
                    || s.len() > 63
                    || s.starts_with('-')
                    || s.ends_with('-')
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
            || !path_prefix.starts_with('/')
            || !path_prefix.ends_with('/')
            || path_prefix.contains('?')
            || path_prefix.contains('%')
            || revision == 0
        {
            return Err(Error::Policy);
        }
        validate_relative(&path_prefix[1..])?;
        Ok(Self {
            host: host.into(),
            prefix: path_prefix.into(),
            header,
            attachment,
            revision,
        })
    }
}
/// Supplied exclusively by authenticated host authority; never deserialize client facts.
pub struct UseEvidence {
    pub subject: String,
    pub policy_revision: u64,
    pub authorization_id: String,
}
pub struct AuthorizedUse {
    policy: DestinationPolicy,
    evidence: UseEvidence,
}
impl AuthorizedUse {
    pub fn new(policy: DestinationPolicy, evidence: UseEvidence) -> Result<Self, Error> {
        if evidence.policy_revision != policy.revision
            || evidence.subject.is_empty()
            || evidence.authorization_id.is_empty()
        {
            return Err(Error::Authority);
        }
        Ok(Self { policy, evidence })
    }
}
/// Internal trusted seam. Resolution must use the gate's binding, not client IDs.
/// Callbacks must be bounded: synchronous blocking cannot be preempted by Tokio.
pub trait TrustedAuthority: Sync {
    fn audit_before_act(&self, evidence: &UseEvidence) -> Result<(), Error>;
    fn resolve_for_use(&self, evidence: &UseEvidence) -> Result<SecretValue, Error>;
}
pub struct JsonResult {
    pub json: serde_json::Value,
    pub bytes: usize,
}
impl fmt::Debug for JsonResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JsonResult([REDACTED])")
    }
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid destination policy")]
    Policy,
    #[error("invalid relative request")]
    Input,
    #[error("credential use denied")]
    Authority,
    #[error("audit unavailable")]
    Audit,
    #[error("destination denied or unavailable")]
    Destination,
    #[error("credential unavailable")]
    Credential,
    #[error("upstream request failed")]
    Upstream,
    #[error("bounded request deadline exceeded")]
    Deadline,
    #[error("upstream output rejected")]
    Output,
}

fn validate_relative(input: &str) -> Result<(), Error> {
    if input.len() > 4096
        || input.starts_with('/')
        || input.contains(['#', '\\', ':'])
        || input.bytes().any(|b| b <= 32 || b >= 127)
    {
        return Err(Error::Input);
    }
    let mut iter = input.bytes();
    while let Some(b) = iter.next() {
        if b == b'%' {
            let hi = iter
                .next()
                .and_then(|v| char::from(v).to_digit(16))
                .ok_or(Error::Input)?;
            let lo = iter
                .next()
                .and_then(|v| char::from(v).to_digit(16))
                .ok_or(Error::Input)?;
            let v = (hi * 16 + lo) as u8;
            // Reject nested encoding and encoded separators/dots even in query.
            if v <= 32 || v >= 127 || matches!(v, b'.' | b'/' | b'\\' | b'%' | b':' | b'#') {
                return Err(Error::Input);
            }
        }
    }
    let path = input.split('?').next().ok_or(Error::Input)?;
    if path.split('/').any(|s| s == "." || s == "..") {
        return Err(Error::Input);
    }
    Ok(())
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a, b, c, _] = v.octets();
            !matches!(a, 0 | 10 | 127 | 224..=255)
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 169 && b == 254)
                && !(a == 172 && (16..=31).contains(&b))
                && !(a == 192 && (b == 0 || b == 168 || (b == 88 && c == 99) || (b == 2)))
                && !(a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                && !(a == 203 && b == 0 && c == 113)
                && !(a == 168 && b == 63 && c == 129) // Azure platform virtual IP
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && s[1] < 0x200)
                && !(s[0] == 0x2001 && s[1] == 0xdb8)
                && !(s[0] == 0x2002) // 6to4 embeds IPv4
                && !(s[0] == 0x3fff) // documentation
        }
    }
}

pub async fn get_json(
    authority: &impl TrustedAuthority,
    lease: AuthorizedUse,
    relative_path_query: &str,
) -> Result<JsonResult, Error> {
    validate_relative(relative_path_query)?;
    tokio::time::timeout(DEADLINE, execute(authority, lease, relative_path_query))
        .await
        .map_err(|_| Error::Deadline)?
}
async fn execute(
    authority: &impl TrustedAuthority,
    lease: AuthorizedUse,
    relative: &str,
) -> Result<JsonResult, Error> {
    let policy = lease.policy;
    let url = Url::parse(&format!(
        "https://{}{}{}",
        policy.host, policy.prefix, relative
    ))
    .map_err(|_| Error::Input)?;
    if url.host_str() != Some(&policy.host)
        || !url.path().starts_with(&policy.prefix)
        || url.port_or_known_default() != Some(443)
    {
        return Err(Error::Input);
    }
    authority
        .audit_before_act(&lease.evidence)
        .map_err(|_| Error::Audit)?;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((policy.host.as_str(), 443))
        .await
        .map_err(|_| Error::Destination)?
        .collect();
    if addresses.is_empty() || addresses.len() > 64 || addresses.iter().any(|a| !public_ip(a.ip()))
    {
        return Err(Error::Destination);
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(DEADLINE)
        .connect_timeout(Duration::from_secs(5))
        .resolve_to_addrs(&policy.host, &addresses)
        .build()
        .map_err(|_| Error::Upstream)?;
    let secret = authority
        .resolve_for_use(&lease.evidence)
        .map_err(|_| Error::Credential)?;
    let raw = secret.expose_to_mediator();
    let prefix = match policy.attachment {
        Attachment::Bearer => "Bearer ",
        Attachment::Raw => "",
    };
    let buffer = Zeroizing::new(format!("{prefix}{raw}"));
    let mut header = HeaderValue::from_str(&buffer).map_err(|_| Error::Credential)?;
    header.set_sensitive(true);
    let name = match policy.header {
        CredentialHeader::Authorization => "authorization",
        CredentialHeader::ReviewedXApiKey => "x-api-key",
    };
    let response = client
        .get(url)
        .header(name, header)
        .send()
        .await
        .map_err(|_| Error::Upstream)?;
    read_json(response, raw).await
}
async fn read_json(mut response: reqwest::Response, secret: &str) -> Result<JsonResult, Error> {
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > CAP as u64)
    {
        return Err(Error::Output);
    }
    let mime = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next());
    if mime != Some("application/json") {
        return Err(Error::Output);
    }
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Upstream)? {
        if chunk.len() > CAP - body.len() {
            return Err(Error::Output);
        }
        body.extend_from_slice(&chunk);
    }
    if body.windows(secret.len()).any(|w| w == secret.as_bytes()) {
        return Err(Error::Output);
    }
    let json: serde_json::Value = serde_json::from_slice(&body).map_err(|_| Error::Output)?;
    if echoed(&json, secret) {
        return Err(Error::Output);
    }
    Ok(JsonResult {
        json,
        bytes: body.len(),
    })
}
fn echoed(value: &serde_json::Value, secret: &str) -> bool {
    match value {
        serde_json::Value::String(s) => s.contains(secret),
        serde_json::Value::Array(a) => a.iter().any(|v| echoed(v, secret)),
        serde_json::Value::Object(o) => o
            .iter()
            .any(|(k, v)| k.contains(secret) || echoed(v, secret)),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
