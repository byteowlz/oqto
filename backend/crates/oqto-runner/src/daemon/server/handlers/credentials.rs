use super::super::super::Runner;
use crate::protocol::RunnerResponse;

/// Only runner control requests reach this handler. The authenticated host must
/// supply Account; this route is deliberately absent from agent capabilities.
pub(super) async fn manage(
    runner: &Runner,
    request: oqto_protocol::CredentialManageRequest,
) -> RunnerResponse {
    RunnerResponse::CredentialsResult(runner.credentials.manage(request).await)
}
