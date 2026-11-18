//! Real signature/admission/router/PersonalBackend path, in-process HTTP only.
//! No socket/TLS/listener/startup/deployed-runtime proof is claimed here.
use super::*;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use oqto_personal::{Grants, PairingAuthority, SessionAccess};
use oqto_personal_wire::{
    Enrollment, OQTO_PERSONAL_V1, SignedRequest,
    actions::{ACTION_PATH, HostAction},
};
use std::sync::Arc;
use tower::ServiceExt;

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
async fn enroll(authority: &PairingAuthority, observe: bool, seed: u8) -> Result<String> {
    let ticket = authority
        .issue_ticket(&Grants {
            sessions: if observe {
                SessionAccess::Observe
            } else {
                SessionAccess::None
            },
            ..Grants::default()
        })
        .await?;
    let key = SigningKey::from_bytes(&[seed; 32]);
    let mut proof = Enrollment {
        token: ticket.token,
        name: "Synthetic observer".into(),
        public_key: STANDARD.encode(key.verifying_key().to_bytes()),
        signature: String::new(),
    };
    proof.signature = STANDARD.encode(
        key.sign(&proof.signing_bytes_for(OQTO_PERSONAL_V1))
            .to_bytes(),
    );
    authority.enroll(&proof).await
}
fn proof(device: &str, seed: u8) -> Result<SignedRequest> {
    let mut request = SignedRequest {
        device_id: device.into(),
        timestamp: now(),
        nonce: uuid::Uuid::new_v4().to_string(),
        payload: serde_json::to_string(&HostAction::SessionsList)?,
        signature: String::new(),
    };
    request.signature = STANDARD.encode(
        SigningKey::from_bytes(&[seed; 32])
            .sign(&request.signing_bytes_for(OQTO_PERSONAL_V1))
            .to_bytes(),
    );
    Ok(request)
}
async fn dispatch(router: &axum::Router, proof: &SignedRequest) -> Result<(u16, Value)> {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(ACTION_PATH)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(proof)?))?,
        )
        .await?;
    let status = response.status().as_u16();
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024).await?;
    Ok((status, serde_json::from_slice(&body)?))
}
async fn router(
    fixture: &Fixture,
    source: bool,
    authority: Arc<PairingAuthority>,
) -> Result<axum::Router> {
    let ingest = if source {
        let (handle, release) = crate::daemon::jsonl_ingest::spawn_paused_inventory_for_test(
            fixture.source.home.clone(),
            "synthetic-owner".into(),
            Arc::new(fixture.source.clone()),
        );
        release
            .send(())
            .map_err(|_| anyhow::anyhow!("Synthetic discovery release failed"))?;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !handle.catalog_ready() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        handle
    } else {
        crate::daemon::jsonl_ingest::spawn_for_test(
            fixture.source.home.clone(),
            "synthetic-owner".into(),
        )
    };
    let backend = Arc::new(crate::daemon::server::PersonalBackend::new(
        authority.clone(),
        None,
        fixture.source.home.clone(),
        fixture.source.workspace_roots().to_vec(),
        false,
        ingest,
        crate::personal_catalog::NativeRoots {
            session_root: fixture.source.session_root().to_owned(),
            socket_root: fixture.source.socket_root().to_owned(),
        },
    ));
    Ok(oqto_personal::actions::action_router(authority, backend))
}
fn expected_ids(fixture: &Fixture) -> HashSet<String> {
    fixture
        .rows
        .iter()
        .map(|row| row.platform_id.clone())
        .collect()
}
fn response_ids(data: &Value) -> HashSet<String> {
    data["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn actual_signed_sessions_list_some_and_none_keep_root_schema_and_exact65_ids() -> Result<()>
{
    let _serial = super::super::super::tests::SERIAL.lock().await;
    let fixture = Fixture::new(65).await?;
    fixture.edge()?;
    let authority = Arc::new(
        PairingAuthority::open_private(&fixture.source.home.join("synthetic-pairing")).await?,
    );
    let observer = enroll(&authority, true, 71).await?;
    for source in [false, true] {
        let router = router(&fixture, source, authority.clone()).await?;
        let (status, data) = dispatch(&router, &proof(&observer, 71)?).await?;
        assert_eq!(status, 200, "source={source}");
        assert_eq!(data["sessions"].as_array().unwrap().len(), 65);
        assert_eq!(response_ids(&data), expected_ids(&fixture));
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ExistingStrictRoot {
            sessions: Vec<Value>,
            read_only: bool,
        }
        let strict: ExistingStrictRoot = serde_json::from_value(data.clone())?;
        assert!(strict.read_only);
        assert_eq!(strict.sessions.len(), 65);
        let edges: Vec<_> = data["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row.get("relations").is_some())
            .collect();
        assert_eq!(edges.len(), usize::from(source));
        if source {
            assert_eq!(edges[0]["relations"][0]["kind"], "delegation");
        }
    }
    Ok(())
}

#[tokio::test]
async fn real_signed_denial_tamper_replay_and_revocation_do_not_inherit_metadata_access()
-> Result<()> {
    let _serial = super::super::super::tests::SERIAL.lock().await;
    let fixture = Fixture::new(2).await?;
    fixture.edge()?;
    let authority = Arc::new(
        PairingAuthority::open_private(&fixture.source.home.join("synthetic-pairing")).await?,
    );
    let observer = enroll(&authority, true, 72).await?;
    let denied = enroll(&authority, false, 73).await?;
    let router = router(&fixture, true, authority.clone()).await?;
    assert_eq!(dispatch(&router, &proof(&denied, 73)?).await?.0, 403);
    let mut tampered = proof(&observer, 72)?;
    tampered.payload.push(' ');
    assert_eq!(dispatch(&router, &tampered).await?.0, 401);
    let request = proof(&observer, 72)?;
    let (status, data) = dispatch(&router, &request).await?;
    assert_eq!(status, 200);
    assert_eq!(response_ids(&data), expected_ids(&fixture));
    assert_eq!(dispatch(&router, &request).await?.0, 401);
    assert!(authority.revoke(&observer).await?);
    assert_ne!(dispatch(&router, &proof(&observer, 72)?).await?.0, 200);
    Ok(())
}
