use super::super::super::Runner;
use crate::protocol::RunnerResponse;

/// Only runner control requests reach this handler. The authenticated host must
/// supply Account; this route is deliberately absent from agent capabilities.
pub(super) async fn manage(
    runner: &Runner,
    request: oqto_protocol::CredentialManageRequest,
) -> RunnerResponse {
    RunnerResponse::CredentialsResult(runner.credentials.manage(request).await)
}

pub(super) async fn describe(runner: &Runner, account: String, id: String) -> RunnerResponse {
    match runner.credentials.describe(account, id).await {
        Ok(value) => RunnerResponse::CredentialDescribed(value),
        Err(_) => crate::daemon::server::error_response(
            crate::protocol::ErrorCode::InvalidRequest,
            "Credential unavailable",
        ),
    }
}

struct AuditedSnapshot {
    event: String,
    account: String,
    secret: oqto_secrets::SecretValue,
}
impl oqto_credential_http::TrustedAuthority for AuditedSnapshot {
    fn audit_before_act(
        &self,
        evidence: &oqto_credential_http::UseEvidence,
    ) -> Result<(), oqto_credential_http::Error> {
        // Constructed only after the async canonical append succeeded. This
        // callback validates the receipt binding; it is not a no-op audit.
        if evidence.authorization_id == self.event && evidence.subject == self.account {
            Ok(())
        } else {
            Err(oqto_credential_http::Error::Audit)
        }
    }
    fn resolve_for_use(
        &self,
        evidence: &oqto_credential_http::UseEvidence,
    ) -> Result<oqto_secrets::SecretValue, oqto_credential_http::Error> {
        self.audit_before_act(evidence)?;
        Ok(self.secret.clone())
    }
}

pub(super) async fn http(
    runner: &Runner,
    request: oqto_protocol::credential_connections::CredentialHttpDispatch,
) -> RunnerResponse {
    match execute(runner, request).await {
        Ok(json) => RunnerResponse::CredentialHttpResult { json },
        Err(_) => crate::daemon::server::error_response(
            crate::protocol::ErrorCode::InvalidRequest,
            "Credential request denied or unavailable",
        ),
    }
}

async fn execute(
    runner: &Runner,
    request: oqto_protocol::credential_connections::CredentialHttpDispatch,
) -> anyhow::Result<serde_json::Value> {
    use anyhow::{Context, ensure};
    use oqto_credential_http::{
        Attachment, AuthorizedUse, CredentialHeader, DestinationPolicy, UseEvidence,
    };
    static REQUESTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(8);
    let _permit = REQUESTS
        .try_acquire()
        .context("credential request capacity")?;
    ensure!(
        request.policy.port == 443 && request.policy.method == "GET",
        "unsupported operation"
    );
    let relative = request
        .path
        .strip_prefix(&request.policy.path_prefix)
        .context("path outside grant")?;
    let header = match request.policy.header.as_str() {
        "authorization" => CredentialHeader::Authorization,
        "x-api-key" => CredentialHeader::ReviewedXApiKey,
        _ => anyhow::bail!("unsupported header"),
    };
    let attachment = match request.policy.attachment_prefix.as_str() {
        "" => Attachment::Raw,
        "Bearer " => Attachment::Bearer,
        _ => anyhow::bail!("unsupported attachment"),
    };
    let policy = DestinationPolicy::new(
        &request.policy.host,
        &request.policy.path_prefix,
        header,
        attachment,
        u64::from(request.connection_revision),
    )?;
    let event = format!("{}.dispatch", request.audit_event_id);
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .context("runner home unavailable")?;
    oqto_history::oqto_log::host_actions::append(
        &home,
        &request.workspace_id,
        &oqto_history::oqto_log::host_actions::HostActionRecord {
            event_id: event.clone(),
            account_id: request.account_id.clone(),
            action: "invoke".into(),
            connection_id: request.connection_id,
            connection_revision: Some(request.connection_revision),
            instance_id: Some(request.instance_id),
            slot: Some(request.slot),
            content_digest: Some(request.content_digest),
        },
    )
    .await?;
    let secret = runner
        .credentials
        .resolve_dispatch(request.account_id.clone(), request.credential)
        .await?;
    let lease = AuthorizedUse::new(
        policy,
        UseEvidence {
            subject: request.account_id.clone(),
            policy_revision: u64::from(request.connection_revision),
            authorization_id: event.clone(),
        },
    )?;
    let authority = AuditedSnapshot {
        event,
        account: request.account_id,
        secret,
    };
    Ok(oqto_credential_http::get_json(&authority, lease, relative)
        .await?
        .json)
}
