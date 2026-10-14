//! Protected credential control: no secrets in CLI arguments or diagnostics.
use anyhow::{Result, bail};
use clap::Subcommand;
use oqto_protocol::credentials::{
    CredentialCommand, CredentialManageResult, CredentialSecretInput,
};
use std::io::Read;

#[derive(Debug, Subcommand)]
pub enum CredentialsCommand {
    Status,
    List,
    Init {
        #[arg(long)]
        stdin: bool,
    },
    Unlock {
        #[arg(long)]
        stdin: bool,
    },
    Lock,
    /// Create by default; updates require the revision returned by list.
    Set {
        id: String,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        stdin: bool,
    },
    Delete {
        id: String,
        #[arg(long)]
        expected_revision: u64,
    },
}

fn validate_server(server: &str, socket: Option<&str>) -> Result<reqwest::Url> {
    if socket.is_some() {
        bail!("credentials require authenticated HTTP, not an admin socket");
    }
    let url = reqwest::Url::parse(server)
        .map_err(|_| anyhow::anyhow!("invalid credential server URL"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("credential server URL must not contain userinfo, query, or fragment");
    }
    let loopback = url
        .host_str()
        .and_then(|host| {
            host.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .ok()
        })
        .is_some_and(|ip| ip.is_loopback());
    // localhost is not an actual address: DNS must not decide whether secrets are safe.
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        bail!("credentials require HTTPS or an actual loopback HTTP address");
    }
    Ok(url)
}

fn read_exact_secret(reader: &mut impl Read) -> Result<CredentialSecretInput> {
    let mut value = String::new();
    reader
        .read_to_string(&mut value)
        .map_err(|_| anyhow::anyhow!("could not read secret input"))?;
    Ok(CredentialSecretInput::new(value))
}

fn secret(stdin: bool) -> Result<CredentialSecretInput> {
    if stdin {
        read_exact_secret(&mut std::io::stdin().lock())
    } else {
        rpassword::prompt_password("Secret: ")
            .map(CredentialSecretInput::new)
            .map_err(|_| anyhow::anyhow!("could not read hidden secret input"))
    }
}

pub async fn handle(
    server: &str,
    socket: Option<&str>,
    command: &CredentialsCommand,
    json: bool,
) -> Result<()> {
    let socket_env = std::env::var("OQTO_ADMIN_SOCKET").ok();
    let url = validate_server(server, socket.or(socket_env.as_deref()))?;
    let token = std::env::var("OQTO_AUTH_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let user = std::env::var("OQTO_DEV_USER")
        .ok()
        .filter(|s| !s.is_empty());
    if token.is_none() && user.is_none() {
        bail!("credential HTTP authentication is not configured");
    }
    // Disable redirects, including HTTPS downgrades and cross-host token/secret forwarding.
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| anyhow::anyhow!("could not construct credential HTTP client"))?;
    let client = super::OqtoClient {
        transport: super::OqtoTransport::Http {
            base_url: url.as_str().trim_end_matches('/').into(),
            client: http,
        },
        auth_token: token,
        dev_user: user,
    };
    let request = match command {
        CredentialsCommand::Status => None,
        CredentialsCommand::List => Some(CredentialCommand::List),
        CredentialsCommand::Init { stdin } => Some(CredentialCommand::Initialize {
            passphrase: secret(*stdin)?,
        }),
        CredentialsCommand::Unlock { stdin } => Some(CredentialCommand::Unlock {
            passphrase: secret(*stdin)?,
        }),
        CredentialsCommand::Lock => Some(CredentialCommand::Lock),
        CredentialsCommand::Set {
            id,
            expected_revision,
            stdin,
        } => Some(CredentialCommand::Put {
            id: id.clone(),
            value: secret(*stdin)?,
            expected_revision: *expected_revision,
        }),
        CredentialsCommand::Delete {
            id,
            expected_revision,
        } => Some(CredentialCommand::Delete {
            id: id.clone(),
            expected_revision: *expected_revision,
        }),
    };
    let result = exchange(&client, request.as_ref()).await?;
    // Do not echo server-provided free-form codes: the stable error surface is local.
    if result.code.is_some() {
        bail!("credential operation rejected by server");
    }
    if json {
        println!(
            "{}",
            serde_json::to_string(&result)
                .map_err(|_| anyhow::anyhow!("could not encode credential metadata"))?
        );
    } else {
        println!(
            "{}",
            serde_json::to_string(&result.status)
                .map_err(|_| anyhow::anyhow!("could not encode credential status"))?
        );
        for entry in result.entries {
            println!(
                "{} revision={}",
                serde_json::to_string(&entry.id)
                    .map_err(|_| anyhow::anyhow!("could not encode credential ID"))?,
                entry.revision
            );
        }
    }
    Ok(())
}

async fn exchange(
    client: &super::OqtoClient,
    command: Option<&CredentialCommand>,
) -> Result<CredentialManageResult> {
    let response = match command {
        None => client.get("/credentials").await,
        Some(command) => client.post_json("/credentials/manage", command).await,
    }
    .map_err(|_| anyhow::anyhow!("credential HTTP request failed"))?;
    if !response.status().is_success() {
        bail!(
            "credential HTTP request rejected (status {})",
            response.status().as_u16()
        );
    }
    serde_json::from_slice(&response.body)
        .map_err(|_| anyhow::anyhow!("invalid credential metadata response"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn http_contract_and_sanitized_errors() -> Result<()> {
        use std::io::{Read, Write};
        for (status, body) in [
            (
                200,
                r#"{"status":"ready","entries":[{"id":"demo","revision":2}],"code":null}"#,
            ),
            (400, "synthetic-secret"),
            (200, "synthetic-secret"),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            let address = listener.local_addr()?;
            let worker = std::thread::spawn(move || -> std::io::Result<String> {
                let (mut socket, _) = listener.accept()?;
                socket.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
                let mut received = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = socket.read(&mut buf)?;
                    received.extend_from_slice(&buf[..n]);
                    if n == 0 {
                        break;
                    }
                    if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&received[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|v| v.parse::<usize>().ok())
                            })
                            .unwrap_or_default();
                        if received.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                write!(
                    socket,
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )?;
                Ok(String::from_utf8_lossy(&received).into_owned())
            });
            let client = super::super::OqtoClient {
                transport: super::super::OqtoTransport::Http {
                    base_url: format!("http://{address}/api"),
                    client: reqwest::Client::new(),
                },
                auth_token: Some("test-auth".into()),
                dev_user: None,
            };
            let result = exchange(
                &client,
                Some(&CredentialCommand::Put {
                    id: "demo".into(),
                    value: CredentialSecretInput::new("synthetic-secret\n".into()),
                    expected_revision: Some(1),
                }),
            )
            .await;
            let wire = worker
                .join()
                .map_err(|_| anyhow::anyhow!("mock failed"))??;
            assert!(wire.starts_with("POST /api/credentials/manage "));
            assert!(wire.contains("Bearer test-auth"));
            let payload: serde_json::Value = serde_json::from_str(
                wire.split_once("\r\n\r\n")
                    .ok_or_else(|| anyhow::anyhow!("missing body"))?
                    .1,
            )?;
            assert_eq!(payload["value"], "synthetic-secret\n");
            assert_eq!(payload["expected_revision"], 1);
            match result {
                Ok(metadata) => assert_eq!(metadata.entries[0].revision, 2),
                Err(error) => assert!(!format!("{error:?}").contains("synthetic-secret")),
            }
        }
        Ok(())
    }

    #[test]
    fn exact_stdin() -> Result<()> {
        let secret = read_exact_secret(&mut " value\n\n".as_bytes())?;
        assert_eq!(secret.expose_for_protected_transport(), " value\n\n");
        Ok(())
    }
    #[test]
    fn transport_is_fail_closed() {
        for url in [
            "ssh://host",
            "stdin://",
            "http://example.com/api",
            "http://localhost/api",
            "https://user:private@host/api",
            "https://host/api?private",
        ] {
            let error = validate_server(url, None)
                .expect_err("unsafe transport")
                .to_string();
            assert!(!error.contains("private"));
        }
        assert!(validate_server("http://127.0.0.1/api", None).is_ok());
        assert!(validate_server("http://[::1]/api", None).is_ok());
        assert!(validate_server("https://host/api", Some("socket")).is_err());
    }
    #[test]
    fn revision_and_secret_arguments() {
        use clap::Parser;
        assert!(
            super::super::Cli::try_parse_from(["oqtoctl", "credentials", "delete", "id"]).is_err()
        );
        assert!(
            super::super::Cli::try_parse_from([
                "oqtoctl",
                "credentials",
                "set",
                "id",
                "--value",
                "secret"
            ])
            .is_err()
        );
    }
}
