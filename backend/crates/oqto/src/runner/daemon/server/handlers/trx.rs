use super::super::*;

pub(crate) async fn handle_request(runner: &Runner, req: RunnerRequest) -> RunnerResponse {
    match req {
        RunnerRequest::TrxList(r) => runner.trx_list(r).await,
        RunnerRequest::TrxCreate(r) => runner.trx_create(r).await,
        RunnerRequest::TrxUpdate(r) => runner.trx_update(r).await,
        RunnerRequest::TrxClose(r) => runner.trx_close(r).await,
        _ => error_response(ErrorCode::InvalidRequest, "Invalid trx request"),
    }
}
