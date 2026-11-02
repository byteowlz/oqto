//! Host-side client for the existing pinned HTTPS + signed personal protocol.
//! Never put this credential in a brain/hands sandbox or command arguments.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use clap::Subcommand;
use ed25519_dalek::{Signer, SigningKey};
use oqto_personal_wire::{
    Enrollment, OQTO_PERSONAL_V1, PairingQr, SignedRequest,
    actions::{ACTION_PATH, HostAction},
};
use rand::TryRngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{TlsConnector, rustls::pki_types::ServerName};

#[derive(Debug, Subcommand)]
pub enum PeerCommand {
    /// Fetch authorized Session media into a NEW local file, not model context.
    Media {
        #[arg(long)]
        identity: PathBuf,
        #[arg(long)]
        session_id: String,
        #[arg(long)]
        sha256: String,
        #[arg(long, value_parser=["thumb", "full"], default_value="thumb")]
        variant: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Consume an owner-provided QR file; creates a NEW private identity file.
    Enroll {
        #[arg(long)]
        qr: PathBuf,
        #[arg(long)]
        identity: PathBuf,
        #[arg(long)]
        name: String,
    },
    /// Ask for exact command consent; reuse request-key on a transport retry.
    Request {
        #[arg(long)]
        identity: PathBuf,
        #[arg(long)]
        request_key: String,
        #[arg(long)]
        session_label: String,
        #[arg(long)]
        command: String,
    },
    Run {
        #[arg(long)]
        identity: PathBuf,
        id: String,
    },
    Status {
        #[arg(long)]
        identity: PathBuf,
        id: String,
    },
    Cancel {
        #[arg(long)]
        identity: PathBuf,
        id: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    url: String,
    pin: String,
    host_id: String,
    device_id: String,
    secret: String,
}
fn now() -> Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    )?)
}

pub async fn run(command: PeerCommand) -> Result<Value> {
    if let PeerCommand::Enroll { qr, identity, name } = command {
        let bytes = std::fs::read(qr)?;
        let qr = PairingQr::parse(&bytes, now()?)?;
        ensure!(
            !name.trim().is_empty() && name.len() <= 120 && !name.contains(['\n', '\r']),
            "invalid device label"
        );
        let parent = identity.parent().context("identity parent required")?;
        let meta = std::fs::symlink_metadata(parent)?;
        ensure!(
            meta.is_dir()
                && meta.uid() == rustix::process::geteuid().as_raw()
                && meta.mode() & 0o077 == 0,
            "identity parent must be an owned private directory"
        );
        // Reserve before enrolling; never overwrite a previously paired identity.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&identity)?;
        let outcome = async {
            let mut seed = [0u8; 32];
            rand::rngs::OsRng
                .try_fill_bytes(&mut seed)
                .context("generate peer key")?;
            let key = SigningKey::from_bytes(&seed);
            let mut enrollment = Enrollment {
                token: qr.enrollment_token().into(),
                name,
                public_key: STANDARD.encode(key.verifying_key().to_bytes()),
                signature: String::new(),
            };
            enrollment.signature = STANDARD.encode(
                key.sign(&enrollment.signing_bytes_for(OQTO_PERSONAL_V1))
                    .to_bytes(),
            );
            let reply = post(
                qr.origin(),
                qr.certificate_sha256(),
                oqto_personal_wire::ENROLL_PATH,
                &serde_json::to_vec(&enrollment)?,
            )
            .await?;
            ensure!(
                reply["host_id"] == qr.host_id(),
                "enrollment host identity mismatch"
            );
            let device_id = reply["device_id"]
                .as_str()
                .context("missing device identity")?
                .to_owned();
            let saved = Identity {
                url: qr.origin().into(),
                pin: qr.certificate_sha256().into(),
                host_id: qr.host_id().into(),
                device_id: device_id.clone(),
                secret: STANDARD.encode(seed),
            };
            use std::io::Write;
            file.write_all(&serde_json::to_vec(&saved)?)?;
            file.sync_all()?;
            Ok::<_, anyhow::Error>(
                serde_json::json!({"host_id":saved.host_id,"device_id":device_id}),
            )
        }
        .await;
        if outcome.is_err() {
            drop(file);
            std::fs::remove_file(identity)?;
        }
        return outcome;
    }
    if let PeerCommand::Media {
        identity,
        session_id,
        sha256,
        variant,
        output,
    } = command
    {
        let full = variant == "full";
        let action = HostAction::SessionMedia {
            session_id,
            sha256: sha256.clone(),
            variant: if full {
                oqto_personal_wire::actions::SessionMediaVariant::Full
            } else {
                oqto_personal_wire::actions::SessionMediaVariant::Thumb
            },
        };
        let mut result = signed(
            &identity,
            &action,
            if full { 12 * 1024 * 1024 } else { 256 * 1024 },
        )
        .await?;
        ensure!(
            result["source_sha256"] == sha256,
            "media source hash mismatch"
        );
        let bytes = STANDARD.decode(
            result["data_base64"]
                .as_str()
                .context("missing media data")?,
        )?;
        use sha2::{Digest, Sha256};
        ensure!(
            bytes.len() <= 8 * 1024 * 1024
                && result["sha256"] == hex::encode(Sha256::digest(&bytes))
                && result["byte_size"].as_u64() == Some(bytes.len() as u64),
            "media integrity mismatch"
        );
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&output)?;
        use std::io::Write;
        file.write_all(&bytes)?;
        file.sync_all()?;
        let metadata = result.as_object_mut().context("invalid media response")?;
        metadata.remove("data_base64");
        metadata.insert("output_path".into(), serde_json::to_value(output)?);
        return Ok(result);
    }
    let (path, action) = match command {
        PeerCommand::Request {
            identity,
            request_key,
            session_label,
            command,
        } => (
            identity,
            HostAction::HandsRequest {
                request_key,
                session_label,
                command,
            },
        ),
        PeerCommand::Run { identity, id } => (identity, HostAction::HandsRun { id }),
        PeerCommand::Status { identity, id } => (identity, HostAction::HandsStatus { id }),
        PeerCommand::Cancel { identity, id } => (identity, HostAction::HandsCancel { id }),
        PeerCommand::Enroll { .. } | PeerCommand::Media { .. } => {
            anyhow::bail!("operation already handled")
        }
    };
    signed(&path, &action, 256 * 1024).await
}

async fn signed(path: &Path, action: &HostAction, response_limit: usize) -> Result<Value> {
    let identity = load(path)?;
    let seed = STANDARD.decode(&identity.secret)?;
    let key = SigningKey::from_bytes(seed.as_slice().try_into().context("invalid peer key")?);
    let mut proof = SignedRequest {
        device_id: identity.device_id,
        timestamp: now()?,
        nonce: uuid::Uuid::new_v4().to_string(),
        payload: serde_json::to_string(&action)?,
        signature: String::new(),
    };
    proof.signature = STANDARD.encode(
        key.sign(&proof.signing_bytes_for(OQTO_PERSONAL_V1))
            .to_bytes(),
    );
    post_limited(
        &identity.url,
        &identity.pin,
        ACTION_PATH,
        &serde_json::to_vec(&proof)?,
        response_limit,
    )
    .await
}

fn load(path: &Path) -> Result<Identity> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file()
            && meta.uid() == rustix::process::geteuid().as_raw()
            && meta.mode() & 0o077 == 0
            && meta.nlink() == 1
            && meta.len() <= 8192,
        "unsafe peer identity file"
    );
    Ok(serde_json::from_reader(file)?)
}

async fn post(origin: &str, pin: &str, path: &str, body: &[u8]) -> Result<Value> {
    post_limited(origin, pin, path, body, 256 * 1024).await
}
async fn post_limited(
    origin: &str,
    pin: &str,
    path: &str,
    body: &[u8],
    response_limit: usize,
) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(15), async {
        // Origins originate in the validated QR; reject altered saved values.
        ensure!(origin.starts_with("https://"),"HTTPS required");
        let address=origin.strip_prefix("https://").context("HTTPS required")?.trim_end_matches('/');
        ensure!(!address.contains(['/', '?', '#', '@', '\r', '\n']),"invalid peer origin");
        let (host,port)=if address.starts_with('[') {
            let (host,rest)=address.split_once(']').context("invalid IPv6 origin")?;
            (host.trim_start_matches('['), rest.strip_prefix(':').unwrap_or("443").parse::<u16>()?)
        } else {
            match address.rsplit_once(':') { Some((host,port))=>(host,port.parse::<u16>()?),None=>(address,443) }
        };
        let config=oqto_personal_wire::tls::pinned_client_config(pin)?;
        let socket=TcpStream::connect((host,port)).await?;
        let mut tls=TlsConnector::from(Arc::new(config)).connect(ServerName::try_from(host.to_owned())?,socket).await?;
        let header=format!("POST {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
        tls.write_all(header.as_bytes()).await?; tls.write_all(body).await?;
        let mut response=Vec::new(); tls.take(response_limit as u64+1).read_to_end(&mut response).await?;
        ensure!(response.len()<=response_limit,"peer response exceeds bound");
        let response=std::str::from_utf8(&response)?;
        let (headers,body)=response.split_once("\r\n\r\n").context("invalid peer response")?;
        let status=headers.split_whitespace().nth(1).context("missing HTTP status")?.parse::<u16>()?;
        ensure!(status==200,"Runner returned HTTP {status}");
        Ok::<_,anyhow::Error>(serde_json::from_str(body)?)
    }).await.context("peer request timed out; query the SAME invocation, do not replay")?
}
