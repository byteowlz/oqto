use super::super::*;

pub(crate) async fn handle_request(runner: &Runner, req: RunnerRequest) -> RunnerResponse {
    match req {
        RunnerRequest::SearchMemories(r) => runner.search_memories(r).await,
        RunnerRequest::AddMemory(r) => runner.add_memory(r).await,
        RunnerRequest::DeleteMemory(r) => runner.delete_memory(r).await,
        _ => error_response(ErrorCode::InvalidRequest, "Invalid memories request"),
    }
}
