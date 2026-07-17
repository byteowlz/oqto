//! Bounded, transport-independent framing for the runner wire.
//!
//! The current compatibility format is one JSON value followed by `\n`.
//! Keeping framing here allows the Unix migration to remain wire-compatible
//! while ensuring every transport applies the same size and parse limits.

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// Maximum encoded runner frame accepted during the compatibility migration.
pub const MAX_RUNNER_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Encode one bounded JSON-line compatibility frame.
pub fn encode_json_frame<T>(value: &T) -> Result<Vec<u8>>
where
    T: Serialize + ?Sized,
{
    let mut encoded = serde_json::to_vec(value).context("serializing runner frame")?;
    if encoded.len() > MAX_RUNNER_FRAME_BYTES {
        anyhow::bail!(
            "runner frame exceeds {} byte limit (encoded bytes: {})",
            MAX_RUNNER_FRAME_BYTES,
            encoded.len()
        );
    }
    encoded.push(b'\n');
    Ok(encoded)
}

/// Write one JSON-line compatibility frame.
pub async fn write_json_frame<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
    T: Serialize + ?Sized,
{
    let encoded = encode_json_frame(value)?;
    writer
        .write_all(&encoded)
        .await
        .context("writing runner frame")?;
    Ok(())
}

/// Read and decode one bounded JSON-line compatibility frame.
///
/// Returns `Ok(None)` only for a clean EOF before any frame bytes. EOF in the
/// middle of a frame is rejected so reconnect logic cannot mistake a truncated
/// command or event for a valid message.
pub async fn read_json_frame<R, T>(reader: &mut R) -> Result<Option<T>>
where
    R: AsyncBufRead + Unpin + ?Sized,
    T: DeserializeOwned,
{
    let mut frame = Vec::new();

    loop {
        let available = reader.fill_buf().await.context("reading runner frame")?;
        if available.is_empty() {
            if frame.is_empty() {
                return Ok(None);
            }
            anyhow::bail!("runner connection closed with a truncated frame");
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        let payload_bytes = newline.unwrap_or(take);

        if frame.len().saturating_add(payload_bytes) > MAX_RUNNER_FRAME_BYTES {
            anyhow::bail!("runner frame exceeds {} byte limit", MAX_RUNNER_FRAME_BYTES);
        }

        frame.extend_from_slice(&available[..payload_bytes]);
        reader.consume(take);

        if newline.is_some() {
            break;
        }
    }

    let value = serde_json::from_slice(&frame).context("parsing runner frame")?;
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{RunnerRequest, RunnerResponse};
    use serde::{Deserialize, Serialize};
    use tokio::io::{AsyncWriteExt, BufReader};

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct Message {
        value: String,
    }

    #[tokio::test]
    async fn frame_round_trips_over_duplex_stream() -> Result<()> {
        let (client, server) = tokio::io::duplex(256);
        let mut client = BufReader::new(client);
        let mut server = BufReader::new(server);
        let expected = Message {
            value: "transport-neutral".to_string(),
        };

        write_json_frame(server.get_mut(), &expected).await?;
        let actual: Option<Message> = read_json_frame(&mut client).await?;
        assert_eq!(actual, Some(expected));
        Ok(())
    }

    #[tokio::test]
    async fn canonical_request_response_works_over_in_process_stream() -> Result<()> {
        let (client, server) = tokio::io::duplex(256);
        let (client_read, mut client_write) = tokio::io::split(client);
        let (server_read, mut server_write) = tokio::io::split(server);
        let mut client_read = BufReader::new(client_read);
        let mut server_read = BufReader::new(server_read);

        let server_task = tokio::spawn(async move {
            let request: RunnerRequest = read_json_frame(&mut server_read)
                .await?
                .ok_or_else(|| anyhow::anyhow!("client closed before request"))?;
            let response = match request {
                RunnerRequest::Ping => RunnerResponse::Pong,
                other => anyhow::bail!("unexpected conformance request: {other:?}"),
            };
            write_json_frame(&mut server_write, &response).await
        });

        write_json_frame(&mut client_write, &RunnerRequest::Ping).await?;
        let response: Option<RunnerResponse> = read_json_frame(&mut client_read).await?;
        assert!(matches!(response, Some(RunnerResponse::Pong)));
        server_task.await.context("joining conformance server")??;
        Ok(())
    }

    #[tokio::test]
    async fn malformed_frame_is_rejected() -> Result<()> {
        let (mut writer, reader) = tokio::io::duplex(64);
        writer.write_all(b"{not-json}\n").await?;
        let mut reader = BufReader::new(reader);

        let error = read_json_frame::<_, Message>(&mut reader)
            .await
            .expect_err("malformed JSON must fail");
        assert!(error.to_string().contains("parsing runner frame"));
        Ok(())
    }

    #[tokio::test]
    async fn truncated_frame_is_rejected() -> Result<()> {
        let (mut writer, reader) = tokio::io::duplex(64);
        writer.write_all(br#"{"value":"partial"}"#).await?;
        writer.shutdown().await?;
        let mut reader = BufReader::new(reader);

        let error = read_json_frame::<_, Message>(&mut reader)
            .await
            .expect_err("truncated frame must fail");
        assert!(error.to_string().contains("truncated frame"));
        Ok(())
    }

    #[tokio::test]
    async fn oversized_frame_is_rejected_without_unbounded_growth() -> Result<()> {
        let oversized = vec![b'x'; MAX_RUNNER_FRAME_BYTES + 1];
        let mut reader = BufReader::new(std::io::Cursor::new(oversized));

        let error = read_json_frame::<_, Message>(&mut reader)
            .await
            .expect_err("oversized frame must fail");
        assert!(error.to_string().contains("exceeds"));
        Ok(())
    }
}
