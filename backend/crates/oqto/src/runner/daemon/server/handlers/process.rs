use super::super::*;

pub(crate) async fn handle_request(runner: &Runner, req: RunnerRequest) -> RunnerResponse {
    match req {
        RunnerRequest::SpawnProcess(r) => runner.spawn_process(r, false).await,
        RunnerRequest::SpawnRpcProcess(r) => {
            runner
                .spawn_process(
                    SpawnProcessRequest {
                        id: r.id,
                        binary: r.binary,
                        args: r.args,
                        cwd: r.cwd,
                        env: r.env,
                        sandboxed: r.sandboxed,
                    },
                    true,
                )
                .await
        }
        RunnerRequest::KillProcess(r) => runner.kill_process(r).await,
        RunnerRequest::GetStatus(r) => runner.get_status(r).await,
        RunnerRequest::ListProcesses => runner.list_processes().await,
        RunnerRequest::WriteStdin(r) => runner.write_stdin(r).await,
        RunnerRequest::ReadStdout(r) => runner.read_stdout(r).await,
        RunnerRequest::SubscribeStdout(_) => error_response(
            ErrorCode::Internal,
            "SubscribeStdout must be handled via streaming",
        ),
        _ => error_response(ErrorCode::InvalidRequest, "Invalid process request"),
    }
}
