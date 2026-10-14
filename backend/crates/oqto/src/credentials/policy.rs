use anyhow::{Context, Result, ensure};
use oqto_protocol::credential_connections::ConnectionPolicy;

pub fn validate_policy(policy: &ConnectionPolicy) -> Result<()> {
    let host = &policy.host;
    ensure!(
        host.len() <= 253 && host.contains('.') && host == &host.to_ascii_lowercase(),
        "invalid DNS host"
    );
    ensure!(
        host.parse::<std::net::IpAddr>().is_err(),
        "IP destinations unsupported"
    );
    let terminal = host
        .rsplit('.')
        .next()
        .context("DNS host has no terminal label")?;
    ensure!(
        !terminal.bytes().all(|byte| byte.is_ascii_digit()) && !terminal.starts_with("0x"),
        "numeric URL host aliases unsupported"
    );
    ensure!(
        host.split('.').all(|label| !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')),
        "invalid DNS host"
    );
    ensure!(
        policy.port == 443 && policy.method == "GET",
        "unsupported Connection operation"
    );
    ensure!(
        matches!(policy.header.as_str(), "authorization" | "x-api-key"),
        "unsupported credential header"
    );
    ensure!(
        matches!(policy.attachment_prefix.as_str(), "" | "Bearer "),
        "unsupported attachment"
    );
    ensure!(
        policy.path_prefix.ends_with('/'),
        "Connection path prefix must end in '/' for the reviewed HTTPS GET mediator"
    );
    validate_path(&policy.path_prefix)?;
    Ok(())
}

pub fn validate_path(path: &str) -> Result<()> {
    ensure!(path.starts_with('/') && !path.contains("//") && path.len() <= 2048 && path.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~')), "unsupported request path");
    ensure!(
        !path.split('/').any(|part| matches!(part, "." | "..")),
        "path traversal"
    );
    Ok(())
}

pub fn permits_path(prefix: &str, path: &str) -> bool {
    path == prefix
        || (path.starts_with(prefix)
            && (prefix.ends_with('/') || path.as_bytes().get(prefix.len()) == Some(&b'/')))
}
