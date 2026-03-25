//! Extracted channel handlers from ws_multiplexed.

use super::*;

pub(super) async fn handle_files_command(
    cmd: FilesWsCommand,
    user_id: &str,
    state: &AppState,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    let id = match &cmd {
        FilesWsCommand::Tree { id, .. }
        | FilesWsCommand::Read { id, .. }
        | FilesWsCommand::Write { id, .. }
        | FilesWsCommand::List { id, .. }
        | FilesWsCommand::Stat { id, .. }
        | FilesWsCommand::Delete { id, .. }
        | FilesWsCommand::CreateDirectory { id, .. }
        | FilesWsCommand::Rename { id, .. }
        | FilesWsCommand::Copy { id, .. }
        | FilesWsCommand::Move { id, .. }
        | FilesWsCommand::CopyToWorkspace { id, .. }
        | FilesWsCommand::WatchFiles { id, .. }
        | FilesWsCommand::UnwatchFiles { id, .. } => id.clone(),
    };

    // Handle WatchFiles/UnwatchFiles early -- they need conn_state access.
    if let FilesWsCommand::WatchFiles { id, workspace_path } = cmd {
        return handle_watch_files(id, &workspace_path, user_id, state, conn_state).await;
    }
    if let FilesWsCommand::UnwatchFiles { id, workspace_path } = cmd {
        return handle_unwatch_files(id, &workspace_path, conn_state).await;
    }

    // CopyToWorkspace has its own dual-workspace handling; dispatch it early.
    if let FilesWsCommand::CopyToWorkspace {
        id,
        source_workspace_path,
        source_path,
        target_workspace_path,
        target_path,
    } = cmd
    {
        return handle_copy_to_workspace(
            id,
            &source_workspace_path,
            &source_path,
            &target_workspace_path,
            &target_path,
            user_id,
            state,
        )
        .await;
    }

    let workspace_path_owned: Option<String> = match &cmd {
        FilesWsCommand::Tree { workspace_path, .. }
        | FilesWsCommand::Read { workspace_path, .. }
        | FilesWsCommand::Write { workspace_path, .. }
        | FilesWsCommand::List { workspace_path, .. }
        | FilesWsCommand::Stat { workspace_path, .. }
        | FilesWsCommand::Delete { workspace_path, .. }
        | FilesWsCommand::CreateDirectory { workspace_path, .. }
        | FilesWsCommand::Rename { workspace_path, .. }
        | FilesWsCommand::Copy { workspace_path, .. }
        | FilesWsCommand::Move { workspace_path, .. } => workspace_path.clone(),
        FilesWsCommand::CopyToWorkspace { .. }
        | FilesWsCommand::WatchFiles { .. }
        | FilesWsCommand::UnwatchFiles { .. } => unreachable!(),
    };
    let workspace_path = workspace_path_owned.as_deref();

    let workspace_root = match resolve_workspace_root(workspace_path) {
        Ok(path) => path,
        Err(err) => {
            return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
        }
    };

    let mut linux_username = state
        .linux_users
        .as_ref()
        .map(|lu| lu.linux_username(user_id))
        .unwrap_or_else(|| user_id.to_string());

    // Shared workspace file operations must use the workspace Linux user.
    if state.linux_users.is_some()
        && let Some(ws_path) = workspace_path
        && let Ok(ExecutionTarget::SharedWorkspace { workspace_id }) =
            resolve_target_for_workspace_path(state, user_id, ws_path).await
    {
        let Some(sw) = state.shared_workspaces.as_ref() else {
            return Some(WsEvent::Files(FilesWsEvent::Error {
                id,
                error: "Shared workspace service not configured".to_string(),
            }));
        };

        match sw.get(&workspace_id, user_id).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                return Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: "Access denied: shared workspace membership required".to_string(),
                }));
            }
            Err(err) => {
                return Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: format!("Failed to resolve shared workspace access: {}", err),
                }));
            }
        }

        match sw.linux_user_for_id(&workspace_id).await {
            Ok(Some(owner_linux_user)) => {
                linux_username = owner_linux_user;
            }
            Ok(None) => {
                return Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: "Shared workspace linux user not found".to_string(),
                }));
            }
            Err(err) => {
                return Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: format!("Failed to resolve shared workspace linux user: {}", err),
                }));
            }
        }
    }

    let is_multi_user = state.linux_users.is_some();
    let user_plane: Arc<dyn UserPlane> = if let Some(endpoint) = state.runner_endpoint.as_ref() {
        match RunnerUserPlane::for_user_with_endpoint(&linux_username, endpoint) {
            Ok(plane) => {
                let base: Arc<dyn UserPlane> = Arc::new(plane);
                Arc::new(MeteredUserPlane::new(
                    base,
                    UserPlanePath::Runner,
                    state.user_plane_metrics.clone(),
                ))
            }
            Err(err) => {
                // SECURITY: In multi-user mode, NEVER fall back to DirectUserPlane.
                // DirectUserPlane runs as the oqto system user which has access to
                // ALL user workspaces. We must only access files through the
                // per-user runner.
                error!(
                    "Failed to create RunnerUserPlane for {}: {:#}",
                    linux_username, err
                );
                return Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: "File access unavailable: user runner not reachable".to_string(),
                }));
            }
        }
    } else if is_multi_user {
        // Multi-user mode without runner endpoint — configuration error.
        error!("Multi-user mode without runner_endpoint configured");
        return Some(WsEvent::Files(FilesWsEvent::Error {
            id,
            error: "File access not configured for multi-user mode".to_string(),
        }));
    } else {
        // Single-user mode is also runner-only.
        match RunnerUserPlane::new_default() {
            Ok(plane) => {
                let base: Arc<dyn UserPlane> = Arc::new(plane);
                Arc::new(MeteredUserPlane::new(
                    base,
                    UserPlanePath::Runner,
                    state.user_plane_metrics.clone(),
                ))
            }
            Err(runner_err) => {
                return Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: format!(
                        "File access unavailable: runner not reachable ({:#})",
                        runner_err
                    ),
                }));
            }
        }
    };

    fn build_tree<'a>(
        user_plane: &'a Arc<dyn crate::user_plane::UserPlane>,
        workspace_root: &'a std::path::Path,
        relative_path: &'a str,
        depth: usize,
        include_hidden: bool,
        traversal: TreeTraversalContext,
        paging: Option<(usize, usize)>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<TreeBuildResult, String>> + Send + 'a>,
    > {
        Box::pin(async move {
            if traversal.should_stop().await {
                return Ok(TreeBuildResult {
                    nodes: Vec::new(),
                    next_offset: None,
                    total_entries: 0,
                });
            }

            let resolved = resolve_workspace_child(workspace_root, relative_path)?;

            let permit = traversal
                .semaphore
                .acquire()
                .await
                .map_err(|_| "tree traversal worker semaphore closed".to_string())?;
            let entries = user_plane
                .list_directory(&resolved, include_hidden)
                .await
                .map_err(|e| {
                    format!("list_directory failed for {}: {:#}", resolved.display(), e)
                })?;
            let entries = sort_dir_entries(entries);
            drop(permit);

            let total_entries = entries.len();
            let (paged_entries, next_offset) = if let Some((offset, limit)) = paging {
                let start = offset.min(total_entries);
                let end = start.saturating_add(limit).min(total_entries);
                let next = (end < total_entries).then_some(end);
                (
                    entries
                        .into_iter()
                        .skip(start)
                        .take(end.saturating_sub(start))
                        .collect::<Vec<_>>(),
                    next,
                )
            } else {
                (entries, None)
            };

            // Separate directories (need recursive fetch) from files (instant)
            let mut file_nodes = Vec::new();
            let mut dir_entries = Vec::new();

            for entry in paged_entries {
                if traversal.should_stop().await {
                    break;
                }
                if !traversal.try_visit_node().await {
                    break;
                }

                let child_path = join_relative_path(relative_path, &entry.name);
                if entry.is_dir && depth > 1 {
                    dir_entries.push((entry, child_path));
                } else {
                    file_nodes.push(map_tree_node(&entry, child_path, None));
                }
            }

            // Fetch all subdirectories concurrently with bounded semaphore.
            let dir_futures: Vec<_> = dir_entries
                .iter()
                .map(|(_, child_path)| {
                    build_tree(
                        user_plane,
                        workspace_root,
                        child_path,
                        depth - 1,
                        include_hidden,
                        traversal.clone(),
                        None,
                    )
                })
                .collect();

            let dir_results = futures::future::join_all(dir_futures).await;

            // Build directory nodes from results, preserving original order
            let mut nodes = Vec::with_capacity(file_nodes.len() + dir_entries.len());
            for ((entry, child_path), result) in dir_entries.into_iter().zip(dir_results) {
                let children = Some(result?.nodes);
                nodes.push(map_tree_node(&entry, child_path, children));
            }
            nodes.append(&mut file_nodes);

            Ok(TreeBuildResult {
                nodes,
                next_offset,
                total_entries,
            })
        })
    }

    fn copy_recursive<'a>(
        user_plane: &'a Arc<dyn crate::user_plane::UserPlane>,
        from_path: &'a std::path::Path,
        to_path: &'a std::path::Path,
        overwrite: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let from_stat = user_plane
                .stat(from_path)
                .await
                .map_err(|e| e.to_string())?;
            if !from_stat.exists {
                return Err("source path does not exist".into());
            }

            let dest_stat = user_plane.stat(to_path).await.map_err(|e| e.to_string())?;
            if dest_stat.exists {
                if !overwrite {
                    return Err("destination already exists".into());
                }
                user_plane
                    .delete_path(to_path, true)
                    .await
                    .map_err(|e| e.to_string())?;
            }

            if from_stat.is_dir {
                user_plane
                    .create_directory(to_path, true)
                    .await
                    .map_err(|e| e.to_string())?;
                let entries = user_plane
                    .list_directory(from_path, true)
                    .await
                    .map_err(|e| e.to_string())?;
                for entry in entries {
                    let child_from = from_path.join(&entry.name);
                    let child_to = to_path.join(&entry.name);
                    copy_recursive(user_plane, &child_from, &child_to, overwrite).await?;
                }
                Ok(())
            } else {
                let content = user_plane
                    .read_file(from_path, None, None)
                    .await
                    .map_err(|e| e.to_string())?;
                user_plane
                    .write_file(to_path, &content.content, true)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
        })
    }

    match cmd {
        FilesWsCommand::Tree {
            id,
            path,
            depth,
            include_hidden,
            offset,
            limit,
            ..
        } => {
            let max_depth = resolve_tree_depth(depth);
            let traversal = TreeTraversalContext::new(
                TREE_MAX_NODES,
                Duration::from_millis(TREE_MAX_TIME_MS),
                TREE_MAX_CONCURRENCY,
            );
            let started = Instant::now();
            let page_offset = offset.unwrap_or(0);
            let page_limit = resolve_tree_page_limit(limit);

            match build_tree(
                &user_plane,
                &workspace_root,
                &path,
                max_depth,
                include_hidden,
                traversal.clone(),
                Some((page_offset, page_limit)),
            )
            .await
            {
                Ok(result) => {
                    let stop_reason = traversal.stop_reason().await;
                    let truncated = Some(stop_reason.is_some() || result.next_offset.is_some());
                    Some(WsEvent::Files(FilesWsEvent::TreeResult {
                        id,
                        path,
                        entries: result.nodes,
                        truncated,
                        stop_reason,
                        visited_nodes: Some(traversal.visited_nodes()),
                        elapsed_ms: Some(started.elapsed().as_millis() as u64),
                        next_offset: result.next_offset,
                        total_entries: Some(result.total_entries),
                    }))
                }
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error { id, error: err })),
            }
        }
        FilesWsCommand::Read { id, path, .. } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match user_plane.read_file(&resolved, None, None).await {
                Ok(content) => {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(content.content);
                    Some(WsEvent::Files(FilesWsEvent::ReadResult {
                        id,
                        path,
                        content: encoded,
                        size: Some(content.size),
                        truncated: Some(content.truncated),
                    }))
                }
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::Write {
            id,
            path,
            content,
            create_parents,
            ..
        } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let decoded = match base64::engine::general_purpose::STANDARD.decode(content) {
                Ok(bytes) => bytes,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error {
                        id,
                        error: format!("invalid base64 content: {}", err),
                    }));
                }
            };
            match user_plane
                .write_file(&resolved, &decoded, create_parents)
                .await
            {
                Ok(()) => {
                    emit_file_bus_event(
                        &state.bus,
                        user_id,
                        workspace_path,
                        "written",
                        serde_json::json!({ "path": &path }),
                    );
                    Some(WsEvent::Files(FilesWsEvent::WriteResult {
                        id,
                        path,
                        success: true,
                    }))
                }
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::List {
            id,
            path,
            include_hidden,
            ..
        } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match user_plane.list_directory(&resolved, include_hidden).await {
                Ok(entries) => Some(WsEvent::Files(FilesWsEvent::ListResult {
                    id,
                    path,
                    entries,
                })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::Stat { id, path, .. } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match user_plane.stat(&resolved).await {
                Ok(stat) => Some(WsEvent::Files(FilesWsEvent::StatResult {
                    id,
                    path,
                    stat: serde_json::to_value(&stat).unwrap_or(Value::Null),
                })),
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::Delete {
            id,
            path,
            recursive,
            ..
        } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match user_plane.delete_path(&resolved, recursive).await {
                Ok(()) => {
                    emit_file_bus_event(
                        &state.bus,
                        user_id,
                        workspace_path,
                        "deleted",
                        serde_json::json!({ "path": &path }),
                    );
                    Some(WsEvent::Files(FilesWsEvent::DeleteResult {
                        id,
                        path,
                        success: true,
                    }))
                }
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::CreateDirectory {
            id,
            path,
            create_parents,
            ..
        } => {
            let resolved = match resolve_workspace_child(&workspace_root, &path) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match user_plane.create_directory(&resolved, create_parents).await {
                Ok(()) => {
                    emit_file_bus_event(
                        &state.bus,
                        user_id,
                        workspace_path,
                        "created",
                        serde_json::json!({ "path": &path, "is_dir": true }),
                    );
                    Some(WsEvent::Files(FilesWsEvent::CreateDirectoryResult {
                        id,
                        path,
                        success: true,
                    }))
                }
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error {
                    id,
                    error: err.to_string(),
                })),
            }
        }
        FilesWsCommand::Rename { id, from, to, .. } => {
            let from_resolved = match resolve_workspace_child(&workspace_root, &from) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let to_resolved = match resolve_workspace_child(&workspace_root, &to) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let copy_result = copy_recursive(&user_plane, &from_resolved, &to_resolved, true).await;
            let result = match copy_result {
                Ok(()) => user_plane
                    .delete_path(&from_resolved, true)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    emit_file_bus_event(
                        &state.bus,
                        user_id,
                        workspace_path,
                        "renamed",
                        serde_json::json!({ "from": &from, "to": &to }),
                    );
                    Some(WsEvent::Files(FilesWsEvent::RenameResult {
                        id,
                        from,
                        to,
                        success: true,
                    }))
                }
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error { id, error: err })),
            }
        }
        FilesWsCommand::Copy {
            id,
            from,
            to,
            overwrite,
            ..
        } => {
            let from_resolved = match resolve_workspace_child(&workspace_root, &from) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let to_resolved = match resolve_workspace_child(&workspace_root, &to) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            match copy_recursive(&user_plane, &from_resolved, &to_resolved, overwrite).await {
                Ok(()) => {
                    emit_file_bus_event(
                        &state.bus,
                        user_id,
                        workspace_path,
                        "copied",
                        serde_json::json!({ "from": &from, "to": &to }),
                    );
                    Some(WsEvent::Files(FilesWsEvent::CopyResult {
                        id,
                        from,
                        to,
                        success: true,
                    }))
                }
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error { id, error: err })),
            }
        }
        FilesWsCommand::Move {
            id,
            from,
            to,
            overwrite,
            ..
        } => {
            let from_resolved = match resolve_workspace_child(&workspace_root, &from) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let to_resolved = match resolve_workspace_child(&workspace_root, &to) {
                Ok(path) => path,
                Err(err) => {
                    return Some(WsEvent::Files(FilesWsEvent::Error { id, error: err }));
                }
            };
            let copy_result =
                copy_recursive(&user_plane, &from_resolved, &to_resolved, overwrite).await;
            let result = match copy_result {
                Ok(()) => user_plane
                    .delete_path(&from_resolved, true)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    emit_file_bus_event(
                        &state.bus,
                        user_id,
                        workspace_path,
                        "moved",
                        serde_json::json!({ "from": &from, "to": &to }),
                    );
                    Some(WsEvent::Files(FilesWsEvent::MoveResult {
                        id,
                        from,
                        to,
                        success: true,
                    }))
                }
                Err(err) => Some(WsEvent::Files(FilesWsEvent::Error { id, error: err })),
            }
        }
        // These are handled by early returns before this match block
        FilesWsCommand::CopyToWorkspace { .. }
        | FilesWsCommand::WatchFiles { .. }
        | FilesWsCommand::UnwatchFiles { .. } => unreachable!(),
    }
}

async fn resolve_terminal_session_owner_for_target(
    state: &AppState,
    user_id: &str,
    target: &ExecutionTarget,
) -> Result<String, String> {
    match target {
        ExecutionTarget::Personal => Ok(user_id.to_string()),
        ExecutionTarget::SharedWorkspace { workspace_id } => {
            let sw = state
                .shared_workspaces
                .as_ref()
                .ok_or_else(|| "Shared workspace service not configured".to_string())?;

            let access = sw
                .get(workspace_id, user_id)
                .await
                .map_err(|e| e.to_string())?;
            if access.is_none() {
                return Err("Access denied: shared workspace membership required".to_string());
            }

            sw.linux_user_for_id(workspace_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "Shared workspace linux user not found".to_string())
        }
    }
}

pub(super) async fn resolve_terminal_session(
    user_id: &str,
    state: &AppState,
    workspace_path: Option<&str>,
    session_id: Option<&str>,
) -> Result<Session, String> {
    info!(
        "resolve_terminal_session: user={}, workspace_path={:?}, session_id={:?}",
        user_id, workspace_path, session_id
    );
    if let Some(session_id) = session_id {
        let owner = match state.session_targets.get(session_id).await {
            Ok(Some(record)) => {
                // Self-heal stale personal scope for shared workspace sessions.
                if matches!(record.scope, SessionTargetScope::Personal)
                    && let Some(workspace_path) = record.workspace_path.clone()
                    && let Ok(ExecutionTarget::SharedWorkspace { workspace_id }) =
                        resolve_target_for_workspace_path(state, user_id, &workspace_path).await
                {
                    let corrected = SessionTargetRecord {
                        session_id: session_id.to_string(),
                        owner_user_id: None,
                        scope: SessionTargetScope::SharedWorkspace,
                        workspace_id: Some(workspace_id.clone()),
                        workspace_path: Some(workspace_path),
                    };
                    let _ = state.session_targets.upsert(&corrected).await;
                    resolve_terminal_session_owner_for_target(
                        state,
                        user_id,
                        &ExecutionTarget::SharedWorkspace { workspace_id },
                    )
                    .await?
                } else {
                    match record.scope {
                        SessionTargetScope::Personal => {
                            if let Some(ref owner_user_id) = record.owner_user_id
                                && owner_user_id != user_id
                            {
                                return Err(
                                    "Access denied: session does not belong to this user".into()
                                );
                            }
                            user_id.to_string()
                        }
                        SessionTargetScope::SharedWorkspace => {
                            let workspace_id = record.workspace_id.ok_or_else(|| {
                                "Invalid session target metadata: missing workspace_id".to_string()
                            })?;
                            resolve_terminal_session_owner_for_target(
                                state,
                                user_id,
                                &ExecutionTarget::SharedWorkspace { workspace_id },
                            )
                            .await?
                        }
                    }
                }
            }
            Ok(None) => user_id.to_string(),
            Err(e) => return Err(format!("Failed to resolve session target: {}", e)),
        };

        let session = state
            .sessions
            .for_user(&owner)
            .get_session(session_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Session not found".to_string())?;
        let session = crate::api::proxy::builder::ensure_session_for_io_proxy(
            state, &owner, session_id, session,
        )
        .await
        .map_err(|_| "Failed to resume session for terminal".to_string())?;
        return Ok(session);
    }

    let workspace_path = workspace_path.ok_or_else(|| "workspace_path is required".to_string())?;
    let target = resolve_target_for_workspace_path(state, user_id, workspace_path)
        .await
        .map_err(|e| e.to_string())?;
    let owner = resolve_terminal_session_owner_for_target(state, user_id, &target).await?;

    let session = state
        .sessions
        .for_user(&owner)
        .get_or_create_io_session_for_workspace(workspace_path)
        .await
        .map_err(|e| e.to_string())?;
    let session_id = session.id.clone();
    let session = crate::api::proxy::builder::ensure_session_for_io_proxy(
        state,
        &owner,
        &session_id,
        session,
    )
    .await
    .map_err(|_| "Failed to resume session for terminal".to_string())?;
    Ok(session)
}

enum TtydConnection {
    Unix(tokio_tungstenite::WebSocketStream<tokio::net::UnixStream>),
    Tcp(
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ),
}

enum TtydConnectionWrite {
    Unix(
        futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<tokio::net::UnixStream>,
            tokio_tungstenite::tungstenite::Message,
        >,
    ),
    Tcp(
        futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            tokio_tungstenite::tungstenite::Message,
        >,
    ),
}

enum TtydConnectionRead {
    Unix(futures::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio::net::UnixStream>>),
    Tcp(
        futures::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
    ),
}

impl TtydConnection {
    fn split(self) -> (TtydConnectionWrite, TtydConnectionRead) {
        match self {
            TtydConnection::Unix(ws) => {
                let (write, read) = ws.split();
                (
                    TtydConnectionWrite::Unix(write),
                    TtydConnectionRead::Unix(read),
                )
            }
            TtydConnection::Tcp(ws) => {
                let (write, read) = ws.split();
                (
                    TtydConnectionWrite::Tcp(write),
                    TtydConnectionRead::Tcp(read),
                )
            }
        }
    }
}

impl TtydConnectionWrite {
    async fn send(
        &mut self,
        msg: tokio_tungstenite::tungstenite::Message,
    ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        match self {
            TtydConnectionWrite::Unix(w) => w.send(msg).await,
            TtydConnectionWrite::Tcp(w) => w.send(msg).await,
        }
    }
}

impl TtydConnectionRead {
    async fn next(
        &mut self,
    ) -> Option<
        Result<tokio_tungstenite::tungstenite::Message, tokio_tungstenite::tungstenite::Error>,
    > {
        match self {
            TtydConnectionRead::Unix(r) => r.next().await,
            TtydConnectionRead::Tcp(r) => r.next().await,
        }
    }
}

async fn connect_ttyd_socket(session_id: &str, ttyd_port: u16) -> anyhow::Result<TtydConnection> {
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let socket_path = ProcessManager::ttyd_socket_path(session_id);
    if socket_path.exists() {
        use tokio::net::UnixStream;
        use tokio_tungstenite::client_async;

        let stream = UnixStream::connect(&socket_path).await?;
        let mut request = "ws://localhost/ws".into_client_request()?;
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            axum::http::HeaderValue::from_static("tty"),
        );
        let (socket, _response) = client_async(request, stream).await?;
        return Ok(TtydConnection::Unix(socket));
    }

    let url = format!("ws://localhost:{}/ws", ttyd_port);
    let mut request = url.into_client_request()?;
    request.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        axum::http::HeaderValue::from_static("tty"),
    );
    let (socket, _response) = connect_async(request).await?;
    Ok(TtydConnection::Tcp(socket))
}

pub(super) async fn start_terminal_task(
    terminal_id: String,
    session_id: String,
    ttyd_port: u16,
    cols: u16,
    rows: u16,
    event_tx: mpsc::UnboundedSender<WsEvent>,
) -> Result<
    (
        mpsc::UnboundedSender<TerminalSessionCommand>,
        tokio::task::JoinHandle<()>,
    ),
    String,
> {
    let (command_tx, mut command_rx) = mpsc::unbounded_channel::<TerminalSessionCommand>();

    let task = tokio::spawn(async move {
        let timeout = crate::api::proxy::builder::DEFAULT_WS_TIMEOUT;
        let start = tokio::time::Instant::now();
        let mut attempts: u32 = 0;
        let socket = loop {
            attempts += 1;
            match connect_ttyd_socket(&session_id, ttyd_port).await {
                Ok(socket) => break socket,
                Err(err) => {
                    if start.elapsed() >= timeout {
                        let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Error {
                            id: None,
                            terminal_id: Some(terminal_id.clone()),
                            error: format!("ttyd not available: {}", err),
                        }));
                        return;
                    }
                }
            }
            let backoff_ms = (attempts.min(20) as u64) * 100;
            tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
        };

        let (mut ttyd_write, mut ttyd_read) = socket.split();

        let init_msg = serde_json::json!({
            "AuthToken": "",
            "columns": cols,
            "rows": rows,
        });
        let init_text = init_msg.to_string();
        if ttyd_write
            .send(tokio_tungstenite::tungstenite::Message::Binary(
                init_text.as_bytes().to_vec().into(),
            ))
            .await
            .is_err()
        {
            let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Error {
                id: None,
                terminal_id: Some(terminal_id.clone()),
                error: "Failed to initialize terminal".into(),
            }));
            return;
        }

        let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Opened {
            id: None,
            terminal_id: terminal_id.clone(),
        }));

        loop {
            tokio::select! {
                Some(cmd) = command_rx.recv() => {
                    match cmd {
                        TerminalSessionCommand::Input(data) => {
                            let mut payload = Vec::with_capacity(1 + data.len());
                            payload.push(b'0');
                            payload.extend_from_slice(data.as_bytes());
                            let _ = ttyd_write.send(tokio_tungstenite::tungstenite::Message::Binary(payload.into())).await;
                        }
                        TerminalSessionCommand::Resize { cols, rows } => {
                            let resize = serde_json::json!({
                                "columns": cols,
                                "rows": rows,
                            });
                            let mut payload = vec![b'1'];
                            payload.extend_from_slice(resize.to_string().as_bytes());
                            let _ = ttyd_write.send(tokio_tungstenite::tungstenite::Message::Binary(payload.into())).await;
                        }
                        TerminalSessionCommand::Close => {
                            let _ = ttyd_write.send(tokio_tungstenite::tungstenite::Message::Close(None)).await;
                            break;
                        }
                    }
                }
                msg = ttyd_read.next() => {
                    match msg {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(data))) => {
                            if data.is_empty() {
                                continue;
                            }
                            let (prefix, payload) = data.split_at(1);
                            if prefix[0] == b'0' {
                                let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
                                let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Output {
                                    terminal_id: terminal_id.clone(),
                                    data_base64: encoded,
                                }));
                            }
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                            let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
                            let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Output {
                                terminal_id: terminal_id.clone(),
                                data_base64: encoded,
                            }));
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => break,
                        Some(Err(_)) | None => break,
                        _ => {}
                    }
                }
            }
        }

        let _ = event_tx.send(WsEvent::Terminal(TerminalWsEvent::Exit { terminal_id }));
    });

    Ok((command_tx, task))
}
