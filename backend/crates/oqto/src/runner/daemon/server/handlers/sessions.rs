use super::super::*;

pub(crate) async fn handle_request(runner: &Runner, req: RunnerRequest) -> RunnerResponse {
    match req {
        RunnerRequest::ListSessions => runner.list_sessions().await,
        RunnerRequest::GetSession(r) => runner.get_session(r).await,
        RunnerRequest::StartSession(r) => runner.start_session(r).await,
        RunnerRequest::StopSession(r) => runner.stop_session(r).await,
        RunnerRequest::ListMainChatSessions => runner.list_main_chat_sessions().await,
        RunnerRequest::GetMainChatMessages(r) => runner.get_main_chat_messages(r).await,
        RunnerRequest::GetWorkspaceChatMessages(r) => runner.get_workspace_chat_messages(r).await,
        RunnerRequest::ListWorkspaceChatSessions(r) => runner.list_workspace_chat_sessions(r).await,
        RunnerRequest::GetWorkspaceChatSession(r) => runner.get_workspace_chat_session(r).await,
        RunnerRequest::GetWorkspaceChatSessionMessages(r) => {
            runner.get_workspace_chat_session_messages(r).await
        }
        RunnerRequest::UpdateWorkspaceChatSession(r) => {
            runner.update_workspace_chat_session(r).await
        }
        _ => error_response(ErrorCode::InvalidRequest, "Invalid sessions request"),
    }
}
