//! Managed history media comes from immutable oqto-log records, not a global
//! cache or the personal native-file admission path.
use super::*;
use std::sync::LazyLock;
static MEDIA_WORKERS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(2)));
impl Runner {
    pub(super) async fn get_workspace_session_media(
        &self,
        req: GetWorkspaceSessionMediaRequest,
    ) -> RunnerResponse {
        let result = async {
            let permit = MEDIA_WORKERS.clone().acquire_owned().await?;
            let home = dirs::home_dir().context("Runner history home unavailable")?;
            let snapshot = oqto_history::oqto_log::ops::media::original(
                &home,
                &req.session_id,
                &req.sha256,
                &req.workspace_scope,
            )
            .await?;
            let bytes = snapshot.bytes().to_vec();
            let variant = req.variant;
            let media = tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
                let _permit = permit;
                match variant {
                    WorkspaceMediaVariant::Full => oqto_media::inline::full(&bytes),
                    WorkspaceMediaVariant::Thumb => {
                        let cache = if cfg!(test) {
                            None
                        } else {
                            Some(oqto_media::cache::config()?)
                        };
                        oqto_media::thumbnail(&bytes, 768, cache)
                    }
                }
            })
            .await??;
            snapshot.validate().await?;
            Ok::<_, anyhow::Error>((media, snapshot.workspace().map(str::to_owned)))
        }
        .await;
        match result {
            Ok((media, workspace_path)) => {
                RunnerResponse::WorkspaceSessionMedia(WorkspaceSessionMediaResponse {
                    session_id: req.session_id,
                    workspace_path,
                    media,
                })
            }
            Err(error) => {
                debug!("Session media refused: {error:#}");
                error_response(ErrorCode::PathNotFound, "Session media unavailable")
            }
        }
    }
}
