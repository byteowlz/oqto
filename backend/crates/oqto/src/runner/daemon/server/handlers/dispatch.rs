use super::super::*;

pub(crate) async fn handle_request(runner: &Runner, req: RunnerRequest) -> RunnerResponse {
    match req {
        RunnerRequest::Ping => RunnerResponse::Pong,
        RunnerRequest::GetCapabilities => runner.get_capabilities().await,
        RunnerRequest::Shutdown => {
            info!("Shutdown requested");
            let _ = runner.shutdown_tx.send(());
            RunnerResponse::ShuttingDown
        }

        req @ (RunnerRequest::SpawnProcess(_)
        | RunnerRequest::SpawnRpcProcess(_)
        | RunnerRequest::KillProcess(_)
        | RunnerRequest::GetStatus(_)
        | RunnerRequest::ListProcesses
        | RunnerRequest::WriteStdin(_)
        | RunnerRequest::ReadStdout(_)
        | RunnerRequest::SubscribeStdout(_)) => super::process::handle_request(runner, req).await,

        req @ (RunnerRequest::ReadFile(_)
        | RunnerRequest::WriteFile(_)
        | RunnerRequest::ListDirectory(_)
        | RunnerRequest::Stat(_)
        | RunnerRequest::DeletePath(_)
        | RunnerRequest::CreateDirectory(_)) => super::files::handle_request(runner, req).await,

        req @ (RunnerRequest::ListSessions
        | RunnerRequest::GetSession(_)
        | RunnerRequest::StartSession(_)
        | RunnerRequest::StopSession(_)
        | RunnerRequest::ListMainChatSessions
        | RunnerRequest::GetMainChatMessages(_)
        | RunnerRequest::GetWorkspaceChatMessages(_)
        | RunnerRequest::ListWorkspaceChatSessions(_)
        | RunnerRequest::GetWorkspaceChatSession(_)
        | RunnerRequest::GetWorkspaceChatSessionMessages(_)
        | RunnerRequest::UpdateWorkspaceChatSession(_)
        | RunnerRequest::RepairWorkspaceChatHistory(_)) => {
            super::sessions::handle_request(runner, req).await
        }

        req @ (RunnerRequest::SearchMemories(_)
        | RunnerRequest::AddMemory(_)
        | RunnerRequest::DeleteMemory(_)) => super::memories::handle_request(runner, req).await,

        req @ (RunnerRequest::PiCreateSession(_)
        | RunnerRequest::PiCloseSession(_)
        | RunnerRequest::PiDeleteSession(_)
        | RunnerRequest::PiNewSession(_)
        | RunnerRequest::PiSwitchSession(_)
        | RunnerRequest::PiListSessions
        | RunnerRequest::PiSubscribe(_)
        | RunnerRequest::PiUnsubscribe(_)
        | RunnerRequest::PiPrompt(_)
        | RunnerRequest::PiSteer(_)
        | RunnerRequest::PiFollowUp(_)
        | RunnerRequest::PiAbort(_)
        | RunnerRequest::PiGetState(_)
        | RunnerRequest::PiGetMessages(_)
        | RunnerRequest::PiGetSessionStats(_)
        | RunnerRequest::PiGetLastAssistantText(_)
        | RunnerRequest::PiSetModel(_)
        | RunnerRequest::PiCycleModel(_)
        | RunnerRequest::PiGetAvailableModels(_)
        | RunnerRequest::PiSetThinkingLevel(_)
        | RunnerRequest::PiCycleThinkingLevel(_)
        | RunnerRequest::PiCompact(_)
        | RunnerRequest::PiSetAutoCompaction(_)
        | RunnerRequest::PiSetSteeringMode(_)
        | RunnerRequest::PiSetFollowUpMode(_)
        | RunnerRequest::PiSetAutoRetry(_)
        | RunnerRequest::PiAbortRetry(_)
        | RunnerRequest::PiFork(_)
        | RunnerRequest::PiGetForkMessages(_)
        | RunnerRequest::PiSetSessionName(_)
        | RunnerRequest::PiExportHtml(_)
        | RunnerRequest::AgentGetCommands(_)
        | RunnerRequest::PiBash(_)
        | RunnerRequest::PiAbortBash(_)
        | RunnerRequest::PiExtensionUiResponse(_)) => super::pi::handle_request(runner, req).await,
    }
}
