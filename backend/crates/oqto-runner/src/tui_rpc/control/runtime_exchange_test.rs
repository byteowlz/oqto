//! Synthetic source peer on the existing verified attachment fixture. Never real Pi.
use super::*;
use oqto_protocol::runtime_interactions::{
    MAX_RUNTIME_FRAME_BYTES, RuntimeOperation as Op, RuntimeRequest,
};

#[derive(Clone, Copy)]
#[repr(usize)]
enum Mode {
    Normal,
    WrongRequest,
    WrongNative,
    WrongEpoch,
    WrongRepresentation,
    WrongFamily,
    HostClaim,
    Oversized,
    SourceStatus,
    RejectNegotiation,
    Silent,
    RevokeSilent,
    ChangedAfterResult,
    LostAfterResult,
    SlowChunks,
    Partial,
    Takeback,
    RevokeAfterResult,
    WrongStatusCorrelation,
    RevokeNegotiation,
}

#[derive(Default)]
pub(super) struct SourceControls {
    pub(super) supported: AtomicBool,
    mode: AtomicUsize,
    queries: AtomicUsize,
    wire: std::sync::Mutex<Vec<Value>>,
}
impl SourceControls {
    fn mode(&self, mode: Mode) {
        self.mode.store(mode as usize, Ordering::SeqCst);
    }

    pub(super) async fn reply(
        &self,
        reader: &mut BufReader<UnixStream>,
        frame: &Value,
        epoch: &std::sync::Mutex<Option<Value>>,
        live: &AtomicBool,
    ) -> Result<()> {
        self.wire.lock().unwrap().push(frame.clone());
        let mode = self.mode.load(Ordering::SeqCst);
        if frame["type"] == "interaction_negotiate" {
            if mode == Mode::RevokeNegotiation as usize {
                live.store(false, Ordering::SeqCst);
            }
            return send(
                reader,
                json!({"type":"interaction_negotiated","version":
                if mode == Mode::RejectNegotiation as usize { Value::Null } else { json!(1) }}),
            )
            .await;
        }
        self.queries.fetch_add(1, Ordering::SeqCst);
        if mode == Mode::Silent as usize || mode == Mode::RevokeSilent as usize {
            if mode == Mode::RevokeSilent as usize {
                live.store(false, Ordering::SeqCst);
            }
            return Ok(());
        }
        if mode == Mode::Partial as usize {
            reader
                .get_mut()
                .write_all(b"{\"type\":\"interaction_source_response\"")
                .await?;
            return Ok(());
        }
        let response = mutate_response(
            mode,
            response_for(&frame["request"]),
            &frame["request"],
            epoch,
            live,
        );
        if mode == Mode::Takeback as usize {
            send(
                reader,
                json!({"type":"lease","owner":"tui","reason":"tui_input"}),
            )
            .await?;
        }
        if mode == Mode::SlowChunks as usize {
            let line = format!("{}\n", serde_json::to_string(&response)?);
            let half = line.len() / 2;
            reader.get_mut().write_all(&line.as_bytes()[..half]).await?;
            tokio::time::sleep(Duration::from_millis(70)).await;
            reader.get_mut().write_all(&line.as_bytes()[half..]).await?;
            return Ok(());
        }
        send(reader, response).await
    }
}

fn mutate_response(
    mode: usize,
    mut response: Value,
    request: &Value,
    epoch: &std::sync::Mutex<Option<Value>>,
    live: &AtomicBool,
) -> Value {
    match mode {
        m if m == Mode::WrongRequest as usize => {
            response["response"]["requestId"] = json!("wrong-request")
        }
        m if m == Mode::WrongNative as usize => {
            response["response"]["source"]["nativeSessionId"] = json!("wrong-native")
        }
        m if m == Mode::WrongEpoch as usize => {
            response["response"]["source"]["runtimeGeneration"] = json!(18)
        }
        m if m == Mode::WrongRepresentation as usize => {
            response["response"]["source"]["runtimeGeneration"] = json!("17")
        }
        m if m == Mode::WrongFamily as usize => {
            response["response"]["result"] =
                json!({"kind":"unavailable","family":"queue","reason":"sdk_engine_absent"})
        }
        m if m == Mode::HostClaim as usize => {
            response["response"]["binding"] = json!({"publicSessionId":"forged"})
        }
        m if m == Mode::Oversized as usize => {
            response["response"]["extra"] = json!("x".repeat(MAX_RUNTIME_FRAME_BYTES))
        }
        m if m == Mode::SourceStatus as usize => {
            response = json!({"type":"interaction_source_status","requestId":request["requestId"],"code":"not_negotiated"})
        }
        m if m == Mode::ChangedAfterResult as usize => *epoch.lock().unwrap() = Some(json!(18)),
        m if m == Mode::LostAfterResult as usize => *epoch.lock().unwrap() = None,
        m if m == Mode::RevokeAfterResult as usize => live.store(false, Ordering::SeqCst),
        m if m == Mode::WrongStatusCorrelation as usize => {
            response = json!({"type":"interaction_source_status","requestId":"wrong-request","code":"not_negotiated"})
        }
        _ => {}
    }
    response
}

fn response_for(request: &Value) -> Value {
    let (family, reason) = match request["operation"]["op"].as_str() {
        Some("command_catalog") => ("commands", "sdk_engine_absent"),
        Some("queue_snapshot") => ("queue", "sdk_engine_absent"),
        Some("error_list") => ("errors", "port_not_configured"),
        _ => ("permissions", "port_not_configured"),
    };
    let result = if request["operation"]["op"] == "capabilities" {
        json!({"kind":"capabilities","capabilities":[{"state":"unavailable","family":"commands","reason":"sdk_engine_absent"}]})
    } else {
        json!({"kind":"unavailable","family":family,"reason":reason})
    };
    json!({"type":"interaction_source_response","response":{"version":1,
        "requestId":request["requestId"],"source":request["source"],"result":result}})
}

fn requests() -> Result<Vec<RuntimeRequest>> {
    let corpus: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../oqto-protocol/src/runtime_interactions/fixtures.json"
    )))?;
    corpus["requests"]
        .as_array()
        .context("requests")?
        .iter()
        .map(|case| serde_json::from_value(case["value"].clone()).map_err(Into::into))
        .collect()
}

async fn source_peer(mode: Mode) -> Result<(Fixture, TuiObserver)> {
    let fixture = Fixture::new(true, Some(true))?;
    *fixture.source_epoch.lock().unwrap() = Some(json!(17));
    fixture.source.supported.store(true, Ordering::SeqCst);
    fixture.source.mode(mode);
    let observer = fixture.observer().await?;
    Ok((fixture, observer))
}

fn capability_query() -> Result<RuntimeRequest> {
    requests()?
        .into_iter()
        .find(|request| {
            matches!(
                request.operation,
                oqto_protocol::runtime_interactions::RuntimeOperation::Capabilities
            )
        })
        .context("capabilities")
}

#[tokio::test]
async fn read_only_source_results_get_only_off_channel_host_binding() -> Result<()> {
    let (fixture, mut observer) = source_peer(Mode::Normal).await?;
    for request in requests()?.into_iter().filter(|request| {
        matches!(
            request.operation,
            oqto_protocol::runtime_interactions::RuntimeOperation::Capabilities
                | oqto_protocol::runtime_interactions::RuntimeOperation::CommandCatalog
                | oqto_protocol::runtime_interactions::RuntimeOperation::QueueSnapshot
                | oqto_protocol::runtime_interactions::RuntimeOperation::ErrorList { .. }
                | oqto_protocol::runtime_interactions::RuntimeOperation::ReceiptGet { .. }
        )
    }) {
        let response = observer.runtime_observe(&request).await?;
        let value = serde_json::to_value(response)?;
        assert_eq!(value["binding"]["publicSessionId"], "public-a");
        assert_eq!(value["binding"]["nativeSessionId"], "native-a");
        assert_eq!(value["binding"]["runtimeGeneration"], 17);
        assert!(value["binding"]["controllerId"].is_null());
    }
    assert_eq!(fixture.source.queries.load(Ordering::SeqCst), 5);
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.leases.load(Ordering::SeqCst), 0);
    for frame in fixture.source.wire.lock().unwrap().iter() {
        assert!(!frame.to_string().contains("public-a"));
        assert!(frame["request"].get("binding").is_none());
    }
    observer.release_input();
    fixture.disconnected().await
}

#[tokio::test]
async fn writes_and_semantic_authority_are_denied_even_with_a_terminal_lease() -> Result<()> {
    let (fixture, mut observer) = source_peer(Mode::Normal).await?;
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    let before = fixture.state_reads.load(Ordering::SeqCst);
    let denied: Vec<_> = requests()?
        .into_iter()
        .filter(|request| {
            matches!(
                request.operation,
                Op::CommandDispatch { .. }
                    | Op::PermissionSnapshot
                    | Op::PermissionDecide { .. }
                    | Op::PolicyRead
                    | Op::PolicyApply { .. }
                    | Op::QueueApply { .. }
            )
        })
        .collect();
    assert_eq!(denied.len(), 6);
    for request in denied {
        assert!(observer.runtime_observe(&request).await.is_err());
    }
    assert_eq!(fixture.state_reads.load(Ordering::SeqCst), before);
    assert_eq!(fixture.source.queries.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn legacy_source_capability_and_revoked_host_deny_before_native_io() -> Result<()> {
    for revoked in [false, true] {
        let fixture = Fixture::new(true, Some(true))?;
        *fixture.source_epoch.lock().unwrap() = Some(json!(17));
        fixture.source.supported.store(revoked, Ordering::SeqCst);
        let mut observer = fixture.observer().await?;
        if revoked {
            fixture.gate.live.store(false, Ordering::SeqCst);
        }
        assert!(
            observer
                .runtime_observe(&capability_query()?)
                .await
                .is_err()
        );
        assert_eq!(fixture.state_reads.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.source.queries.load(Ordering::SeqCst), 0);
    }
    Ok(())
}

async fn refused_source(mode: Mode, expected_queries: usize) -> Result<()> {
    let (fixture, mut observer) = source_peer(mode).await?;
    let query = capability_query()?;
    assert!(observer.runtime_observe(&query).await.is_err());
    assert!(observer.reader.is_none());
    assert!(observer.runtime_observe(&query).await.is_err());
    assert_eq!(
        fixture.source.queries.load(Ordering::SeqCst),
        expected_queries
    );
    fixture.disconnected().await
}

#[tokio::test]
async fn mismatched_malformed_stale_and_refused_results_never_get_host_bindings_or_retry()
-> Result<()> {
    for mode in [
        Mode::WrongRequest,
        Mode::WrongNative,
        Mode::WrongEpoch,
        Mode::WrongRepresentation,
        Mode::WrongFamily,
        Mode::HostClaim,
        Mode::Oversized,
        Mode::SourceStatus,
        Mode::ChangedAfterResult,
        Mode::LostAfterResult,
        Mode::RevokeAfterResult,
        Mode::WrongStatusCorrelation,
    ] {
        refused_source(mode, 1).await?;
    }
    Ok(())
}

#[tokio::test]
async fn failed_negotiation_or_revocation_at_ack_never_dispatches_a_source_request() -> Result<()> {
    refused_source(Mode::RejectNegotiation, 0).await?;
    refused_source(Mode::RevokeNegotiation, 0).await
}

#[tokio::test]
async fn authority_revocation_interrupts_a_silent_source_without_stale_success() -> Result<()> {
    let (fixture, mut observer) = source_peer(Mode::RevokeSilent).await?;
    tokio::time::timeout(
        Duration::from_secs(1),
        observer.runtime_observe(&capability_query()?),
    )
    .await?
    .expect_err("revocation must interrupt before the two-second source deadline");
    fixture.disconnected().await
}

#[tokio::test]
async fn source_timeout_is_terminal_without_automatic_replay() -> Result<()> {
    refused_source(Mode::Silent, 1).await
}

#[tokio::test]
async fn cancellation_mid_frame_reaps_reader_and_owned_control_lease() -> Result<()> {
    let (fixture, mut observer) = source_peer(Mode::Partial).await?;
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    let query = capability_query()?;
    let mut call = Box::pin(observer.runtime_observe(&query));
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            tokio::select! {
                result = &mut call => panic!("source unexpectedly completed: {result:?}"),
                _ = tokio::task::yield_now() => {
                    if fixture.source.queries.load(Ordering::SeqCst) == 1 { break; }
                }
            }
        }
    })
    .await?;
    drop(call);
    assert!(!observer.input_held());
    assert!(observer.reader.is_none());
    fixture.disconnected().await
}

#[tokio::test]
async fn partial_reads_survive_authority_poll_ticks_without_restarting_the_read() -> Result<()> {
    let (fixture, mut observer) = source_peer(Mode::SlowChunks).await?;
    observer.runtime_observe(&capability_query()?).await?;
    observer.release_input();
    fixture.disconnected().await
}

#[tokio::test]
async fn terminal_takeback_closes_only_our_source_connection_and_lease() -> Result<()> {
    let (fixture, mut observer) = source_peer(Mode::Takeback).await?;
    observer
        .acquire_input(fixture.gate.clone(), "device-a")
        .await?;
    assert!(
        observer
            .runtime_observe(&capability_query()?)
            .await
            .is_err()
    );
    assert!(!observer.input_held());
    assert_eq!(fixture.actions.load(Ordering::SeqCst), 0);
    fixture.disconnected().await
}
