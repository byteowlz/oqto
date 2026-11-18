mod support;
use oqto_personal_client::*;
use support::*;

#[tokio::test]
async fn oversized_read_is_bounded_without_exporting_its_response_body() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    peer.state.lock().unwrap().response_bytes = Some(vec![b'x'; 2 * 1024 * 1024 + 1]);
    let error = client.capabilities(HOST).await.unwrap_err();
    assert_eq!(error, ClientError::InvalidResponse);
    assert_eq!(
        serde_json::to_string(&error).unwrap(),
        "\"invalid_response\""
    );
    assert_eq!(peer.count("capabilities"), 1);
}

#[tokio::test]
async fn invalid_mutation_reply_stays_unknown_and_does_not_repeat_input() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    client.request_control(HOST, &attachment.id).await.unwrap();
    peer.state.lock().unwrap().response_bytes = Some(b"synthetic non-JSON reply".to_vec());
    assert_eq!(
        client
            .input(HOST, &attachment.id, SessionInput::Abort)
            .await
            .unwrap_err(),
        ClientError::OutcomeUnknown
    );
    assert_eq!(peer.count("input"), 1);
    assert_eq!(
        client
            .input(HOST, &attachment.id, SessionInput::Abort)
            .await
            .unwrap_err(),
        ClientError::Denied
    );
    assert_eq!(peer.count("input"), 1);
}

#[tokio::test]
async fn blank_or_oversized_prompt_never_crosses_the_signed_transport() {
    let peer = Peer::start().await;
    let (client, _, _) = peer.enrolled().await;
    let attachment = client.attach(HOST, SESSION).await.unwrap();
    client.request_control(HOST, &attachment.id).await.unwrap();
    for message in [" ".into(), "x".repeat(32 * 1024 + 1)] {
        assert_eq!(
            client
                .input(HOST, &attachment.id, SessionInput::Prompt { message })
                .await
                .unwrap_err(),
            ClientError::InvalidInput
        );
    }
    assert_eq!(peer.count("input"), 0);
}
