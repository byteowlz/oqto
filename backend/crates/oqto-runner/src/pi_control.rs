//! Serialized, fail-closed Pi control transaction. No chat send/bookkeeping APIs.
use super::*;
use oqto_protocol::control::{CONTROL_MAX_BYTES, ControlResult, is_control_status};
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub(super) struct Runtime {
    pub pending: PendingResponses,
    pub external_id: SessionExternalId,
}

fn status_waiter(key: &str, request_id: &str) -> String {
    format!("control-status:{key}:{request_id}")
}

/// Deliver only bounded, correlated protocol-v1 statuses to an active transaction.
pub(super) async fn route_status(event: &PiEvent, pending: &PendingResponses) {
    let PiEvent::ExtensionUiRequest(ui) = event else {
        return;
    };
    let (Some(key), Some(text)) = (&ui.status_key, &ui.status_text) else {
        return;
    };
    if ui.method != "setStatus" || !is_control_status(key) || text.len() > CONTROL_MAX_BYTES {
        return;
    }
    let Ok(reply) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    if reply.get("version").and_then(|v| v.as_u64()) != Some(1) {
        return;
    }
    let Some(id) = reply.get("requestId").and_then(|v| v.as_str()) else {
        return;
    };
    if let Some(sender) = pending.write().await.remove(&status_waiter(key, id)) {
        let _ = sender.send(PiResponse {
            id: None,
            success: true,
            data: Some(reply),
            error: None,
        });
    }
}

async fn write_rpc<W: AsyncWrite + Unpin>(writer: &mut W, command: &PiCommand) -> Result<()> {
    let mut bytes = serde_json::to_vec(command)?;
    bytes.push(b'\n');
    writer
        .write_all(&bytes)
        .await
        .context("Write control RPC")?;
    writer.flush().await.context("Flush control RPC")
}

async fn rpc<W: AsyncWrite + Unpin>(
    writer: &mut W,
    pending: &PendingResponses,
    id: &str,
    command: PiCommand,
) -> Result<PiResponse> {
    let (tx, rx) = oneshot::channel();
    pending.write().await.insert(id.to_owned(), tx);
    write_rpc(writer, &command).await?;
    let response = rx.await.context("Control RPC channel closed")?;
    anyhow::ensure!(response.success, "Control RPC rejected");
    Ok(response)
}

/// Exact Pi 1 RPC shape, shared by public discovery and control preflight.
pub(super) fn catalog(commands: &serde_json::Value) -> Result<&[serde_json::Value]> {
    commands
        .get("commands")
        .and_then(|list| list.as_array())
        .map(Vec::as_slice)
        .context("Command catalog missing data.commands array")
}

pub(super) fn discovered(commands: &serde_json::Value, name: &str) -> bool {
    catalog(commands).ok().is_some_and(|list| {
        list.iter().any(|entry| {
            entry.get("name").and_then(|v| v.as_str()) == Some(name)
                && entry.get("source").and_then(|v| v.as_str()) == Some("extension")
        })
    })
}

pub(super) async fn execute<W: AsyncWrite + Unpin>(
    writer: &mut W,
    pending: &PendingResponses,
    expected_native_id: &str,
    request: crate::protocol::AgentControlRequest,
    control_ready: &mut bool,
) -> Result<ControlResult> {
    anyhow::ensure!(*control_ready, "Control runtime unavailable");
    anyhow::ensure!(
        !request.request_id.is_empty()
            && request.request_id.len() <= 128
            && !request.binding_id.is_empty()
            && request.binding_id.len() <= 128,
        "Invalid control transport identity"
    );
    anyhow::ensure!(
        !expected_native_id.is_empty() && expected_native_id != request.session_id,
        "Native Session binding unavailable"
    );
    let correlation = uuid::Uuid::new_v4().to_string();
    let state_id = format!("control-state-{correlation}");
    let discovery_id = format!("control-discovery-{correlation}");
    let dispatch_id = format!("control-dispatch-{correlation}");
    let status_id = status_waiter(request.operation.status_key(), &correlation);
    let args = request
        .operation
        .arguments(request.arguments.clone(), &correlation, &request.binding_id)
        .map_err(anyhow::Error::msg)?;
    let transaction = async {
        let response = rpc(
            writer,
            pending,
            &state_id,
            PiCommand::GetState {
                id: Some(state_id.clone()),
            },
        )
        .await?;
        let native_id = response
            .data
            .as_ref()
            .and_then(|v| v.get("sessionId"))
            .and_then(|v| v.as_str());
        anyhow::ensure!(
            native_id == Some(expected_native_id),
            "Native Session binding changed"
        );
        let response = rpc(
            writer,
            pending,
            &discovery_id,
            PiCommand::GetCommands {
                id: Some(discovery_id.clone()),
            },
        )
        .await?;
        let commands = response.data.context("Missing command catalog")?;
        anyhow::ensure!(
            request
                .operation
                .required_commands()
                .iter()
                .all(|name| discovered(&commands, name)),
            "Extension control unavailable"
        );
        // Subscribe before dispatch: Pi can emit status before its RPC response.
        let (tx, rx) = oneshot::channel();
        pending.write().await.insert(status_id.clone(), tx);
        let message = format!(
            "/{} {}",
            request.operation.command(),
            serde_json::to_string(&args)?
        );
        *control_ready = false;
        let response = rpc(
            writer,
            pending,
            &dispatch_id,
            PiCommand::Prompt {
                id: Some(dispatch_id.clone()),
                message,
                images: None,
                streaming_behavior: None,
            },
        )
        .await?;
        anyhow::ensure!(
            response
                .data
                .as_ref()
                .and_then(|v| v.get("disposition"))
                .and_then(|v| v.as_str())
                == Some("handled"),
            "Control was not handled"
        );
        let reply = rx
            .await
            .context("Control status channel closed")?
            .data
            .context("Missing control acknowledgement")?;
        anyhow::ensure!(
            reply.get("command").and_then(|v| v.as_str()) == Some(request.operation.command())
                && reply.get("ok").and_then(|v| v.as_bool()).is_some(),
            "Invalid control acknowledgement"
        );
        if let Some(scope) = reply
            .get("snapshot")
            .and_then(|v| v.get("scopeId"))
            .or_else(|| reply.get("result").and_then(|v| v.get("scopeId")))
            .or_else(|| {
                reply
                    .get("result")
                    .and_then(|v| v.get("record"))
                    .and_then(|v| v.get("scopeId"))
            })
        {
            anyhow::ensure!(
                scope.as_str() == Some(expected_native_id),
                "Control acknowledgement scope mismatch"
            );
        }
        *control_ready = true;
        Ok(ControlResult {
            binding_id: request.binding_id,
            request_id: request.request_id,
            operation: request.operation,
            native_session_id: expected_native_id.to_owned(),
            reply,
        })
    };
    let result = tokio::time::timeout(Duration::from_secs(10), transaction)
        .await
        .context("Control deadline exceeded");
    // Includes write errors/timeouts; abandoned correlations cannot consume later replies.
    let mut waiters = pending.write().await;
    for id in [state_id, discovery_id, dispatch_id, status_id] {
        waiters.remove(&id);
    }
    result?
}

#[cfg(test)]
#[path = "pi_control_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "pi_control_real_tests.rs"]
mod real_tests;
