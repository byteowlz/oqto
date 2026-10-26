use super::*;

#[test]
fn overlapping_tools_remain_visible_until_their_own_end_and_are_bounded() -> Result<()> {
    let mut tools = std::collections::BTreeMap::new();
    let make = |kind: &str, id: &str| {
        event(json!({"event":kind,"data":{"toolName":"bash","toolCallId":id}})).unwrap()
    };
    update_tool_activity(&mut tools, make("tool_execution_start", "first"))?;
    update_tool_activity(&mut tools, make("tool_execution_start", "second"))?;
    assert_eq!(tools.len(), 2);
    assert_eq!(
        update_tool_activity(&mut tools, make("tool_execution_end", "first"))?,
        Some("bash".into())
    );
    assert_eq!(
        update_tool_activity(&mut tools, make("tool_execution_end", "second"))?,
        None
    );
    for i in 0..64 {
        update_tool_activity(&mut tools, make("tool_execution_start", &i.to_string()))?;
    }
    assert!(update_tool_activity(&mut tools, make("tool_execution_start", "overflow")).is_err());
    assert_eq!(tools.len(), 64);
    Ok(())
}

#[tokio::test]
async fn model_catalog_is_observe_only_scoped_filtered_and_settings_need_input() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let path = fixture
        .authority
        .binding
        .socket_root
        .join(format!("pi-tui-rpc-{}.sock", std::process::id()));
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let mut state = fixture.native_state();
    state["leafId"] = Value::Null;
    state["isStreaming"] = false.into();
    state["model"] = json!({"provider":"synthetic","id":"scoped","name":"Scoped"});
    state["thinkingLevel"] = "high".into();
    let commands = fixture.commands.clone();
    fixture.tasks.push(tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        send(&mut reader, &json!({"type":"hello","v":1,"pid":std::process::id(),"commands":["get_state","get_available_models","get_available_thinking_levels","set_model","set_thinking_level"]})).await.unwrap();
        while let Ok(command) = read_frame(&mut reader).await {
            commands.lock().unwrap().push(command.clone());
            let data = match command["type"].as_str() {
                Some("get_state") => state.clone(),
                Some("get_available_models") => json!({"scoped":[{"provider":"synthetic","id":"scoped","name":"Scoped","apiKey":"synthetic-private","headers":{"Authorization":"never-forward"}}],"models":[{"provider":"synthetic","id":"outside","name":"Other"}]}),
                Some("get_available_thinking_levels") => json!({"levels":["off","high"]}),
                _ => panic!("read-only observer sent mutation"),
            };
            if send(&mut reader, &json!({"type":"response","id":command["id"],"command":command["type"],"success":true,"data":data})).await.is_err() { break; }
        }
    }));
    let mut observer =
        TuiObserver::attach_snapshots(fixture.authority.clone(), "alice", "public-id").await?;
    let options = observer.model_options().await?;
    assert_eq!(options["models"].as_array().unwrap().len(), 1);
    assert_eq!(options["models"][0]["id"], "scoped");
    assert!(!options.to_string().contains("synthetic-private"));
    assert!(!options.to_string().contains("headers"));
    assert_eq!(observer.details()["thinking_level"], "high");
    assert!(
        observer.details()["queue_pending"].is_null(),
        "unsupported queue must not become empty"
    );
    assert!(
        observer
            .input(control::InputAction::SetModel {
                provider: "synthetic".into(),
                model_id: "scoped".into()
            })
            .await
            .is_err()
    );
    assert!(
        fixture
            .commands
            .lock()
            .unwrap()
            .iter()
            .all(|v| v["type"].as_str().is_some_and(|s| s.starts_with("get_")))
    );
    assert_eq!(
        std::fs::read(&fixture.authority.binding.session_file)?,
        fixture.sentinel
    );
    Ok(())
}
