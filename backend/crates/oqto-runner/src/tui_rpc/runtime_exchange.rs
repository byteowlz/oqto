//! Read-only source exchange on an already independently verified native attachment.
//! Raw socket access, typed frames and terminal leases never become host admission.
use super::*;
use oqto_protocol::runtime_interactions::{
    MAX_RUNTIME_FRAME_BYTES, RuntimeOperation, RuntimeRequest, RuntimeResponse, RuntimeResult,
    RuntimeSourceFrame, RuntimeSourceIdentity, RuntimeSourceRequest, RuntimeVersion,
    decode_runtime_source_frame,
};
use std::future::Future;

const SOURCE_DEADLINE: Duration = Duration::from_secs(2);
const MAX_SKIPPED_SOURCE_EVENTS: usize = 4096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Negotiated {
    #[serde(rename = "type")]
    kind: String,
    version: Option<RuntimeVersion>,
}

fn read_only(operation: &RuntimeOperation) -> bool {
    matches!(
        operation,
        RuntimeOperation::Capabilities
            | RuntimeOperation::CommandCatalog
            | RuntimeOperation::QueueSnapshot
            | RuntimeOperation::ErrorList { .. }
            | RuntimeOperation::ReceiptGet { .. }
    )
}

fn revalidate(
    authority: &dyn ObservationAuthority,
    binding: &ObservationBinding,
    lease: Option<&control::InputLease>,
) -> Result<()> {
    authority.revalidate(binding)?;
    if let Some(lease) = lease {
        lease.runtime_controller(binding)?;
    }
    Ok(())
}

async fn fenced<T>(
    check: &impl Fn() -> Result<()>,
    operation: impl Future<Output = Result<T>>,
) -> Result<T> {
    check()?;
    tokio::pin!(operation);
    let mut tick = tokio::time::interval(Duration::from_millis(25));
    loop {
        tokio::select! {
            result = &mut operation => {
                check()?;
                return result;
            },
            _ = tick.tick() => check()?,
        }
    }
}

/// These notifications are private observation input, never a canonical log/replay source.
async fn next_reply(
    reader: &mut BufReader<UnixStream>,
    held: bool,
    check: &impl Fn() -> Result<()>,
) -> Result<Vec<u8>> {
    for _ in 0..MAX_SKIPPED_SOURCE_EVENTS {
        check()?;
        let mut line = read_line(reader).await?;
        check()?;
        let header: FrameKind =
            serde_json::from_slice(&line).map_err(|_| anyhow::anyhow!("Invalid native frame"))?;
        match header.kind.as_deref() {
            Some("event") => {
                let _ = event(serde_json::from_slice(&line)?)?;
            }
            Some("lease") => {
                let frame: Value = serde_json::from_slice(&line)?;
                ensure!(
                    !held || frame["owner"] != "tui",
                    "Terminal reclaimed native input"
                );
            }
            _ => {
                // The shared 256 KiB bound covers JSON bytes, not the JSONL delimiter.
                line.pop(); // read_line has already required a terminating newline.
                return Ok(line);
            }
        }
    }
    bail!("Native source event gap; fresh attachment required")
}

async fn write_packet(
    reader: &mut BufReader<UnixStream>,
    packet: &[u8],
    check: &impl Fn() -> Result<()>,
) -> Result<()> {
    check()?;
    reader.get_mut().write_all(packet).await?;
    check()?;
    reader.get_mut().write_all(b"\n").await?;
    check()
}

async fn exchange(
    reader: &mut BufReader<UnixStream>,
    request: RuntimeSourceRequest,
    held: bool,
    check: &impl Fn() -> Result<()>,
) -> Result<RuntimeResult> {
    let packet = serde_json::to_vec(&RuntimeSourceFrame::Request {
        request: request.clone(),
    })?;
    ensure!(
        packet.len() <= MAX_RUNTIME_FRAME_BYTES,
        "Runtime source request exceeds bound"
    );
    write_packet(
        reader,
        b"{\"type\":\"interaction_negotiate\",\"versions\":[1]}",
        check,
    )
    .await?;
    let negotiated: Negotiated = serde_json::from_slice(&next_reply(reader, held, check).await?)
        .map_err(|_| anyhow::anyhow!("Invalid native source negotiation"))?;
    ensure!(
        negotiated.kind == "interaction_negotiated"
            && negotiated.version == Some(RuntimeVersion::V1),
        "Native source version not negotiated"
    );
    write_packet(reader, &packet, check).await?;
    let line = next_reply(reader, held, check).await?;
    match decode_runtime_source_frame(&line).map_err(anyhow::Error::msg)? {
        RuntimeSourceFrame::Response { response } => {
            ensure!(
                response.matches_request(&request),
                "Native source correlation mismatch"
            );
            Ok(response.result)
        }
        RuntimeSourceFrame::Status { request_id, code } => {
            ensure!(
                request_id.as_ref() == Some(&request.request_id),
                "Uncorrelated native source refusal"
            );
            bail!("Native source refused request: {code:?}")
        }
        RuntimeSourceFrame::Request { .. } => bail!("Unexpected native source request"),
    }
}

impl TuiObserver {
    /// Observe source-owned results and add only CURRENT off-channel host binding.
    /// No writes, semantic decisions, owner config, public-wire action or automatic replay.
    /// Returned source capabilities are engine evidence, not grants or host feature admission.
    pub async fn runtime_observe(&mut self, request: &RuntimeRequest) -> Result<RuntimeResponse> {
        self.authority.revalidate(&self.binding)?;
        ensure!(
            read_only(&request.operation),
            "Runtime source operation is not read-only"
        );
        ensure!(
            self.runtime_source_supported,
            "Native source transport unsupported"
        );
        let before = self.runtime_binding_for(&request.operation).await?;
        let source_request = RuntimeSourceRequest {
            version: request.version,
            request_id: request.request_id.clone(),
            source: RuntimeSourceIdentity {
                native_session_id: before.native_session_id.clone(),
                runtime_generation: before.runtime_generation.clone(),
            },
            operation: request.operation.clone(),
        };
        let result = self
            .exchange_and_bind(request, source_request, before)
            .await;
        if result.is_err() {
            self.release_input();
        }
        result
    }

    async fn exchange_and_bind(
        &mut self,
        request: &RuntimeRequest,
        source: RuntimeSourceRequest,
        before: oqto_protocol::runtime_interactions::RuntimeBinding,
    ) -> Result<RuntimeResponse> {
        // Own reader AND watchdog lease in the future: cancellation cannot retain either.
        let mut reader = self
            .reader
            .take()
            .context("Native attachment disconnected")?;
        let lease = self.input_lease.take();
        let check = || revalidate(self.authority.as_ref(), &self.binding, lease.as_ref());
        let result = tokio::time::timeout(
            SOURCE_DEADLINE,
            fenced(
                &check,
                exchange(&mut reader, source, lease.is_some(), &check),
            ),
        )
        .await
        .context("Native source deadline; do not automatically retry")??;
        self.reader = Some(reader);
        self.input_lease = lease;
        let after = self.runtime_binding_for(&request.operation).await?;
        ensure!(
            before == after,
            "Host/native binding changed; fresh admission required"
        );
        Ok(RuntimeResponse {
            version: request.version,
            request_id: request.request_id.clone(),
            binding: after,
            result,
        })
    }
}
