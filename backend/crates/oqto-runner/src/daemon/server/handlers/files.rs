use super::super::*;

pub(crate) async fn handle_request(runner: &Runner, req: RunnerRequest) -> RunnerResponse {
    match req {
        RunnerRequest::ReadFile(r) => runner.read_file(r).await,
        RunnerRequest::WriteFile(r) => runner.write_file(r).await,
        RunnerRequest::ListDirectory(r) => runner.list_directory(r).await,
        RunnerRequest::Stat(r) => runner.stat(r).await,
        RunnerRequest::DeletePath(r) => runner.delete_path(r).await,
        RunnerRequest::CreateDirectory(r) => runner.create_directory(r).await,
        _ => error_response(ErrorCode::InvalidRequest, "Invalid files request"),
    }
}
