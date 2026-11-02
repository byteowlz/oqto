use super::*;
#[tokio::test]
async fn original_is_scoped_to_exact_public_session_and_never_rewrites_records() -> Result<()> {
    let home = tempfile::tempdir()?;
    let pool = crate::oqto_log::store::open_workspace_pool(home.path(), "/synthetic/work").await?;
    use base64::{Engine, engine::general_purpose::STANDARD};
    let bytes=STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j7ioAAAAASUVORK5CYII=")?;
    let hash = hex::encode(Sha256::digest(&bytes));
    for (internal, public, native) in [
        ("internal-a", "public-a", "native-a"),
        ("internal-b", "public-b", "native-b"),
    ] {
        sqlx::query("INSERT INTO oqto_log_sessions(session_id,platform_id,external_id,user_id,workspace_id) VALUES(?,?,?,'fixture','/synthetic/work')").bind(internal).bind(public).bind(native).execute(&pool).await?;
    }
    let image = serde_json::json!({"type":"image","data":STANDARD.encode(&bytes)});
    let private =
        serde_json::to_string(&serde_json::json!({"type":"custom","private_state":image}))?;
    sqlx::query("INSERT INTO oqto_log_raw_envelopes(raw_id,session_id,source,harness,native_type,source_sequence,received_at,payload_json) VALUES('raw-private','internal-a','fixture','pi','custom',0,datetime('now'),?)").bind(private).execute(&pool).await?;
    let scope = MediaWorkspaceScope::Owned;
    assert!(
        original(home.path(), "public-a", &hash, &scope)
            .await
            .is_err(),
        "knowing a hash must not expose extension/control records"
    );
    sqlx::query(
        "INSERT INTO oqto_log_branches(branch_id,session_id) VALUES('branch-a','internal-a')",
    )
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO oqto_log_turns(turn_id,session_id,branch_id,turn_version,role,status) VALUES('turn-a','internal-a','branch-a',1,'tool','committed')").execute(&pool).await?;
    let payload =
        serde_json::to_string(&serde_json::json!({"role":"toolResult","content":[image]}))?;
    sqlx::query("INSERT INTO oqto_log_messages(message_id,turn_id,seq,kind,role,content,json_payload) VALUES('message-a','turn-a',0,'message','tool','fixture',?)").bind(&payload).execute(&pool).await.context("seed conversation message")?;
    let snapshot = original(home.path(), "public-a", &hash, &scope)
        .await
        .context("read admitted conversation image")?;
    assert_eq!(snapshot.bytes(), bytes.as_slice());
    for id in ["public-b", "native-a", "internal-a"] {
        assert!(original(home.path(), id, &hash, &scope).await.is_err());
    }
    assert!(
        original(
            home.path(),
            "public-a",
            &hash,
            &MediaWorkspaceScope::Exact {
                path: "/foreign/work".into()
            }
        )
        .await
        .is_err()
    );
    assert!(
        original(
            home.path(),
            "public-a",
            &hash,
            &MediaWorkspaceScope::Within {
                root: "/foreign".into()
            }
        )
        .await
        .is_err()
    );
    assert_eq!(
        original(
            home.path(),
            "public-a",
            &hash,
            &MediaWorkspaceScope::Within {
                root: "/synthetic".into()
            }
        )
        .await?
        .bytes(),
        bytes.as_slice()
    );
    let stored: String = sqlx::query_scalar(
        "SELECT json_payload FROM oqto_log_messages WHERE message_id='message-a'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(stored, payload);
    sqlx::query("UPDATE oqto_log_messages SET json_payload=? WHERE message_id='message-a'").bind(serde_json::to_string(&serde_json::json!({"role":"toolResult","content":[{"type":"text","text":"no image"}],"private_extra":image}))?).execute(&pool).await.context("replace conversation image with private extras")?;
    assert!(
        original(home.path(), "public-a", &hash, &scope)
            .await
            .is_err(),
        "private message extras are not conversation media"
    );
    assert!(
        snapshot.validate().await.is_err(),
        "changed source invalidates admitted snapshot"
    );
    Ok(())
}
