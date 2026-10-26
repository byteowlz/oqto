use super::*;

#[test]
fn personal_model_thinking_and_follow_up_are_typed_bounded_commands() -> Result<()> {
    for (command, args, native) in [
        (
            "set_model",
            json!({"provider":"synthetic","model_id":"chat-model"}),
            "set_model",
        ),
        (
            "set_thinking_level",
            json!({"level":"high"}),
            "set_thinking_level",
        ),
        (
            "follow_up",
            json!({"message":"after this run"}),
            "follow_up",
        ),
    ] {
        let input: InputAction = serde_json::from_value(json!({"command":command,"args":args}))?;
        assert_eq!(input.packet()?["type"], native);
    }
    for value in [
        json!({"command":"set_model","args":{"provider":"","model_id":"m"}}),
        json!({"command":"set_model","args":{"provider":"p","model_id":"\n"}}),
        json!({"command":"set_thinking_level","args":{"level":""}}),
        json!({"command":"follow_up","args":{"message":" "}}),
    ] {
        let input: InputAction = serde_json::from_value(value)?;
        assert!(input.packet().is_err());
    }
    assert!(
        serde_json::from_value::<InputAction>(json!({"command":"bash","args":{"command":"no"}}))
            .is_err()
    );
    Ok(())
}
