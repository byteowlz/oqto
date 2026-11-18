use super::*;
use crate::tui_rpc::options::Details;

#[test]
fn context_usage_is_optional_validated_and_secret_filtered() -> Result<()> {
    for (raw, expected) in [
        (
            r#"{"tokens":42,"contextWindow":100,"apiKey":"secret","headers":{"Authorization":"secret"},"baseUrl":"secret"}"#,
            json!({"tokens":42,"context_window":100}),
        ),
        (
            r#"{"tokens":0,"contextWindow":100}"#,
            json!({"tokens":0,"context_window":100}),
        ),
        (
            r#"{"tokens":120,"contextWindow":100}"#,
            json!({"tokens":120,"context_window":100}),
        ),
        (
            r#"{"tokens":null,"contextWindow":100}"#,
            json!({"tokens":null,"context_window":100}),
        ),
        (
            r#"{"contextWindow":100}"#,
            json!({"tokens":null,"context_window":100}),
        ),
        (
            r#"{"tokens":18446744073709551615,"contextWindow":18446744073709551615}"#,
            json!({"tokens":u64::MAX,"context_window":u64::MAX}),
        ),
        ("null", Value::Null),
        ("true", Value::Null),
        ("[]", Value::Null),
        ("{}", Value::Null),
        (r#"{"tokens":42}"#, Value::Null),
        (r#"{"tokens":42,"contextWindow":0}"#, Value::Null),
        (r#"{"tokens":42,"contextWindow":null}"#, Value::Null),
        (r#"{"tokens":-1,"contextWindow":100}"#, Value::Null),
        (r#"{"tokens":42,"contextWindow":-100}"#, Value::Null),
        (r#"{"tokens":1.5,"contextWindow":100}"#, Value::Null),
        (r#"{"tokens":42,"contextWindow":100.5}"#, Value::Null),
        (r#"{"tokens":"42","contextWindow":100}"#, Value::Null),
        (r#"{"tokens":42,"contextWindow":"100"}"#, Value::Null),
        (
            r#"{"tokens":18446744073709551616,"contextWindow":100}"#,
            Value::Null,
        ),
        (
            r#"{"tokens":42,"contextWindow":18446744073709551616}"#,
            Value::Null,
        ),
    ] {
        let details: Details = serde_json::from_str(&format!(
            r#"{{"thinkingLevel":"high","contextUsage":{raw}}}"#
        ))?;
        assert_eq!(details.thinking_level.as_deref(), Some("high"), "{raw}");
        assert_eq!(
            serde_json::to_value(details.context_usage)?,
            expected,
            "{raw}"
        );
    }
    let missing: Details = serde_json::from_value(json!({}))?;
    assert!(missing.context_usage.is_none());
    Ok(())
}

#[tokio::test]
async fn context_usage_replaces_the_source_pair_without_stats_or_input() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let path = fixture
        .authority
        .binding
        .socket_root
        .join(format!("pi-tui-rpc-{}.sock", std::process::id()));
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let base = fixture.native_state();
    let commands = fixture.commands.clone();
    let pairs = vec![
        (
            json!({"tokens":42,"contextWindow":100,"apiKey":"synthetic-secret","headers":{"Authorization":"synthetic-secret"}}),
            json!({"tokens":42,"context_window":100}),
        ),
        (
            json!({"tokens":42,"contextWindow":200}),
            json!({"tokens":42,"context_window":200}),
        ),
        (
            json!({"tokens":42,"contextWindow":80}),
            json!({"tokens":42,"context_window":80}),
        ),
        (
            json!({"tokens":120,"contextWindow":80}),
            json!({"tokens":120,"context_window":80}),
        ),
        (
            json!({"tokens":0,"contextWindow":80}),
            json!({"tokens":0,"context_window":80}),
        ),
        (
            json!({"tokens":null,"contextWindow":80}),
            json!({"tokens":null,"context_window":80}),
        ),
        (
            json!({"tokens":20,"contextWindow":80}),
            json!({"tokens":20,"context_window":80}),
        ),
        (json!({"tokens":-1,"contextWindow":80}), Value::Null),
        (
            json!({"tokens":90,"contextWindow":100}),
            json!({"tokens":90,"context_window":100}),
        ),
        (Value::Null, Value::Null),
    ];
    let states = pairs
        .iter()
        .enumerate()
        .map(|(index, (usage, _))| {
            let mut state = base.clone();
            state["model"] = json!({"provider":"synthetic","id":if index == 0 {"first"} else {"replacement"},"contextWindow":200,"apiKey":"synthetic-secret","baseUrl":"synthetic-secret"});
            if !usage.is_null() {
                state["contextUsage"] = usage.clone();
            }
            state
        })
        .collect::<Vec<_>>();
    fixture.tasks.push(tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        send(&mut reader, &json!({"type":"hello","v":1,"pid":std::process::id(),"commands":["get_state"]})).await.unwrap();
        for state in states {
            let command = read_frame(&mut reader).await.unwrap();
            commands.lock().unwrap().push(command.clone());
            assert_eq!(command["type"], "get_state");
            send(&mut reader, &json!({"type":"response","id":command["id"],"command":"get_state","success":true,"data":state})).await.unwrap();
        }
    }));
    let mut observer =
        TuiObserver::attach_snapshots(fixture.authority.clone(), "alice", "public-id").await?;
    for (index, (_, expected)) in pairs.into_iter().enumerate() {
        if index > 0 {
            observer.snapshot_state().await?;
        }
        let details = observer.details();
        assert_eq!(details["context_usage"], expected, "snapshot {index}");
        assert!(!details.to_string().contains("synthetic-secret"));
        assert!(!details.to_string().contains("headers"));
        assert!(!details.to_string().contains("baseUrl"));
        assert_eq!(
            details["model"]["id"],
            if index == 0 { "first" } else { "replacement" }
        );
    }
    fixture.unchanged()?;
    Ok(())
}
