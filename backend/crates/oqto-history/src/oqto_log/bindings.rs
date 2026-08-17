use sqlx::{FromRow, SqliteConnection};
use thiserror::Error;

const PI_HARNESS: &str = "pi";
const SESSION_BINDING_KIND: &str = "session";
const LEGACY_HARNESS: &str = "unknown";
const LEGACY_PI_BINDING_KIND: &str = "pi-compatible-session";

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct SessionBinding {
    pub binding_id: String,
    pub oqto_session_id: String,
    pub harness: String,
    pub provider_instance: String,
    pub binding_kind: String,
    pub external_id: String,
    pub source: String,
    pub first_seen_at: String,
    pub last_seen_at: Option<String>,
    pub supersedes_binding_id: Option<String>,
    pub extensions_json: String,
}

#[derive(Debug, Clone, Copy)]
pub struct SessionBindingKey<'a> {
    pub harness: &'a str,
    pub provider_instance: &'a str,
    pub binding_kind: &'a str,
    pub external_id: &'a str,
}

impl<'a> SessionBindingKey<'a> {
    pub fn pi_session(external_id: &'a str) -> Self {
        Self {
            harness: PI_HARNESS,
            provider_instance: "",
            binding_kind: SESSION_BINDING_KIND,
            external_id,
        }
    }

    fn legacy_pi_compatible(external_id: &'a str) -> Self {
        Self {
            harness: LEGACY_HARNESS,
            provider_instance: "",
            binding_kind: LEGACY_PI_BINDING_KIND,
            external_id,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NewSessionBinding<'a> {
    pub binding_id: &'a str,
    pub oqto_session_id: &'a str,
    pub key: SessionBindingKey<'a>,
    pub source: &'a str,
    pub first_seen_at: Option<&'a str>,
    pub last_seen_at: Option<&'a str>,
    pub supersedes_binding_id: Option<&'a str>,
    pub extensions_json: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSessionBinding {
    pub binding: SessionBinding,
    /// Internal storage key that owns timeline rows.
    pub session_id: String,
    /// Public Oqto Session identity represented by the binding.
    pub platform_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSessionIdentity {
    pub binding: Option<SessionBinding>,
    pub session_id: String,
    pub platform_id: String,
}

#[derive(Debug, Error)]
#[error(
    "native session identity {harness}/{provider_instance}/{binding_kind}/{external_id} is already bound to {existing_oqto_session_id}, not {requested_oqto_session_id}"
)]
pub struct SessionBindingConflict {
    pub harness: String,
    pub provider_instance: String,
    pub binding_kind: String,
    pub external_id: String,
    pub existing_oqto_session_id: String,
    pub requested_oqto_session_id: String,
}

#[derive(Debug, Error)]
pub enum SessionBindingError {
    #[error("session binding field {field} must not be empty")]
    EmptyField { field: &'static str },
    #[error(
        "provisional identity is forbidden in persisted session binding field {field}: {value}"
    )]
    ProvisionalIdentity { field: &'static str, value: String },
    #[error("persisted public Session identity must start with oqto-: {value}")]
    NonCanonicalSessionIdentity { value: String },
    #[error(transparent)]
    Conflict(Box<SessionBindingConflict>),
    #[error("superseded binding {binding_id} does not exist")]
    MissingSupersededBinding { binding_id: String },
    #[error("superseding binding must preserve the predecessor native identity")]
    SupersessionKeyMismatch,
    #[error(
        "legacy external session identity {external_id} maps to multiple sessions {session_ids:?}; explicit reconciliation required"
    )]
    LegacyConflict {
        external_id: String,
        session_ids: Vec<String>,
    },
    #[error("bound Oqto Session no longer exists: {oqto_session_id}")]
    MissingBoundSession { oqto_session_id: String },
    #[error("session binding storage failed: {0}")]
    Storage(#[from] sqlx::Error),
}

fn require_value(value: &str, field: &'static str) -> Result<(), SessionBindingError> {
    if value.trim().is_empty() {
        return Err(SessionBindingError::EmptyField { field });
    }
    Ok(())
}

fn reject_provisional(value: &str, field: &'static str) -> Result<(), SessionBindingError> {
    if value.starts_with("pending-") || value.starts_with("tmp:") {
        return Err(SessionBindingError::ProvisionalIdentity {
            field,
            value: value.to_string(),
        });
    }
    Ok(())
}

fn validate_binding(input: &NewSessionBinding<'_>) -> Result<(), SessionBindingError> {
    require_value(input.binding_id, "binding_id")?;
    require_value(input.oqto_session_id, "oqto_session_id")?;
    require_value(input.key.harness, "harness")?;
    require_value(input.key.binding_kind, "binding_kind")?;
    require_value(input.key.external_id, "external_id")?;
    require_value(input.source, "source")?;
    reject_provisional(input.oqto_session_id, "oqto_session_id")?;
    reject_provisional(input.key.external_id, "external_id")?;
    if !input.oqto_session_id.starts_with("oqto-") {
        return Err(SessionBindingError::NonCanonicalSessionIdentity {
            value: input.oqto_session_id.to_string(),
        });
    }
    Ok(())
}

async fn root_binding(
    connection: &mut SqliteConnection,
    key: SessionBindingKey<'_>,
) -> Result<Option<SessionBinding>, sqlx::Error> {
    sqlx::query_as::<_, SessionBinding>(
        r#"
        SELECT binding_id, oqto_session_id, harness, provider_instance,
               binding_kind, external_id, source, first_seen_at, last_seen_at,
               supersedes_binding_id, extensions_json
        FROM oqto_log_session_bindings
        WHERE harness = ? AND provider_instance = ? AND binding_kind = ?
          AND external_id = ? AND supersedes_binding_id IS NULL
        LIMIT 1
        "#,
    )
    .bind(key.harness)
    .bind(key.provider_instance)
    .bind(key.binding_kind)
    .bind(key.external_id)
    .fetch_optional(&mut *connection)
    .await
}

pub async fn append_session_binding(
    connection: &mut SqliteConnection,
    input: NewSessionBinding<'_>,
) -> Result<SessionBinding, SessionBindingError> {
    validate_binding(&input)?;

    if let Some(predecessor_id) = input.supersedes_binding_id {
        let predecessor = sqlx::query_as::<_, SessionBinding>(
            r#"
            SELECT binding_id, oqto_session_id, harness, provider_instance,
                   binding_kind, external_id, source, first_seen_at, last_seen_at,
                   supersedes_binding_id, extensions_json
            FROM oqto_log_session_bindings WHERE binding_id = ?
            "#,
        )
        .bind(predecessor_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| SessionBindingError::MissingSupersededBinding {
            binding_id: predecessor_id.to_string(),
        })?;

        if predecessor.harness != input.key.harness
            || predecessor.provider_instance != input.key.provider_instance
            || predecessor.binding_kind != input.key.binding_kind
            || predecessor.external_id != input.key.external_id
        {
            return Err(SessionBindingError::SupersessionKeyMismatch);
        }

        if let Some(existing) = sqlx::query_as::<_, SessionBinding>(
            r#"
            SELECT binding_id, oqto_session_id, harness, provider_instance,
                   binding_kind, external_id, source, first_seen_at, last_seen_at,
                   supersedes_binding_id, extensions_json
            FROM oqto_log_session_bindings WHERE supersedes_binding_id = ? LIMIT 1
            "#,
        )
        .bind(predecessor_id)
        .fetch_optional(&mut *connection)
        .await?
        {
            if existing.oqto_session_id == input.oqto_session_id {
                return Ok(existing);
            }
            return Err(conflict(&input, &existing.oqto_session_id));
        }
    } else if let Some(existing) = root_binding(connection, input.key).await? {
        if existing.oqto_session_id == input.oqto_session_id {
            return Ok(existing);
        }
        return Err(conflict(&input, &existing.oqto_session_id));
    }

    sqlx::query(
        r#"
        INSERT INTO oqto_log_session_bindings (
            binding_id, oqto_session_id, harness, provider_instance,
            binding_kind, external_id, source, first_seen_at, last_seen_at,
            supersedes_binding_id, extensions_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, COALESCE(?, datetime('now')), ?, ?, ?)
        "#,
    )
    .bind(input.binding_id)
    .bind(input.oqto_session_id)
    .bind(input.key.harness)
    .bind(input.key.provider_instance)
    .bind(input.key.binding_kind)
    .bind(input.key.external_id)
    .bind(input.source)
    .bind(input.first_seen_at)
    .bind(input.last_seen_at)
    .bind(input.supersedes_binding_id)
    .bind(input.extensions_json)
    .execute(&mut *connection)
    .await?;

    sqlx::query_as::<_, SessionBinding>(
        r#"
        SELECT binding_id, oqto_session_id, harness, provider_instance,
               binding_kind, external_id, source, first_seen_at, last_seen_at,
               supersedes_binding_id, extensions_json
        FROM oqto_log_session_bindings WHERE binding_id = ?
        "#,
    )
    .bind(input.binding_id)
    .fetch_one(&mut *connection)
    .await
    .map_err(SessionBindingError::from)
}

fn conflict(input: &NewSessionBinding<'_>, existing: &str) -> SessionBindingError {
    SessionBindingError::Conflict(Box::new(SessionBindingConflict {
        harness: input.key.harness.to_string(),
        provider_instance: input.key.provider_instance.to_string(),
        binding_kind: input.key.binding_kind.to_string(),
        external_id: input.key.external_id.to_string(),
        existing_oqto_session_id: existing.to_string(),
        requested_oqto_session_id: input.oqto_session_id.to_string(),
    }))
}

pub async fn list_session_bindings(
    connection: &mut SqliteConnection,
    oqto_session_id: &str,
) -> Result<Vec<SessionBinding>, SessionBindingError> {
    reject_provisional(oqto_session_id, "oqto_session_id")?;
    if !oqto_session_id.starts_with("oqto-") {
        return Err(SessionBindingError::NonCanonicalSessionIdentity {
            value: oqto_session_id.to_string(),
        });
    }
    sqlx::query_as::<_, SessionBinding>(
        r#"
        SELECT binding_id, oqto_session_id, harness, provider_instance,
               binding_kind, external_id, source, first_seen_at, last_seen_at,
               supersedes_binding_id, extensions_json
        FROM oqto_log_session_bindings
        WHERE oqto_session_id = ?
        ORDER BY first_seen_at, binding_id
        "#,
    )
    .bind(oqto_session_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(SessionBindingError::from)
}

pub async fn resolve_session_by_binding(
    connection: &mut SqliteConnection,
    key: SessionBindingKey<'_>,
) -> Result<Option<ResolvedSessionBinding>, SessionBindingError> {
    require_value(key.harness, "harness")?;
    require_value(key.binding_kind, "binding_kind")?;
    require_value(key.external_id, "external_id")?;
    reject_provisional(key.external_id, "external_id")?;

    let binding = sqlx::query_as::<_, SessionBinding>(
        r#"
        WITH RECURSIVE chain(binding_id, depth) AS (
            SELECT binding_id, 0
            FROM oqto_log_session_bindings
            WHERE harness = ? AND provider_instance = ? AND binding_kind = ?
              AND external_id = ? AND supersedes_binding_id IS NULL
            UNION ALL
            SELECT child.binding_id, chain.depth + 1
            FROM oqto_log_session_bindings child
            JOIN chain ON child.supersedes_binding_id = chain.binding_id
        )
        SELECT b.binding_id, b.oqto_session_id, b.harness, b.provider_instance,
               b.binding_kind, b.external_id, b.source, b.first_seen_at,
               b.last_seen_at, b.supersedes_binding_id, b.extensions_json
        FROM chain
        JOIN oqto_log_session_bindings b ON b.binding_id = chain.binding_id
        ORDER BY chain.depth DESC
        LIMIT 1
        "#,
    )
    .bind(key.harness)
    .bind(key.provider_instance)
    .bind(key.binding_kind)
    .bind(key.external_id)
    .fetch_optional(&mut *connection)
    .await?;

    let Some(binding) = binding else {
        return Ok(None);
    };
    let session = sqlx::query_as::<_, (String, String)>(
        "SELECT session_id, platform_id FROM oqto_log_sessions WHERE platform_id = ?",
    )
    .bind(&binding.oqto_session_id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| SessionBindingError::MissingBoundSession {
        oqto_session_id: binding.oqto_session_id.clone(),
    })?;

    Ok(Some(ResolvedSessionBinding {
        binding,
        session_id: session.0,
        platform_id: session.1,
    }))
}

pub async fn resolve_pi_session_identity(
    connection: &mut SqliteConnection,
    external_id: &str,
) -> Result<Option<ResolvedSessionIdentity>, SessionBindingError> {
    for key in [
        SessionBindingKey::pi_session(external_id),
        SessionBindingKey::legacy_pi_compatible(external_id),
    ] {
        if let Some(resolved) = resolve_session_by_binding(connection, key).await? {
            return Ok(Some(ResolvedSessionIdentity {
                binding: Some(resolved.binding),
                session_id: resolved.session_id,
                platform_id: resolved.platform_id,
            }));
        }
    }

    // Rolling-upgrade fallback for rows written before the binding table. More
    // than one row is an identity conflict, never a winner-selection problem.
    let legacy = sqlx::query_as::<_, (String, String)>(
        "SELECT session_id, platform_id FROM oqto_log_sessions WHERE external_id = ? ORDER BY session_id",
    )
    .bind(external_id)
    .fetch_all(&mut *connection)
    .await?;
    match legacy.as_slice() {
        [] => Ok(None),
        [(session_id, platform_id)] => Ok(Some(ResolvedSessionIdentity {
            binding: None,
            session_id: session_id.clone(),
            platform_id: platform_id.clone(),
        })),
        rows => Err(SessionBindingError::LegacyConflict {
            external_id: external_id.to_string(),
            session_ids: rows
                .iter()
                .map(|(session_id, _)| session_id.clone())
                .collect(),
        }),
    }
}

/// Resolve identity for a writer that can recreate a previously deleted
/// Session. Reads remain fail-closed on a dangling immutable binding; writers
/// deterministically recreate the same public owner instead of selecting an
/// older superseded fact or minting a different identity.
pub async fn resolve_pi_session_identity_for_write(
    connection: &mut SqliteConnection,
    external_id: &str,
) -> Result<Option<ResolvedSessionIdentity>, SessionBindingError> {
    match resolve_pi_session_identity(connection, external_id).await {
        Err(SessionBindingError::MissingBoundSession { oqto_session_id }) => {
            Ok(Some(ResolvedSessionIdentity {
                binding: None,
                session_id: oqto_session_id.clone(),
                platform_id: oqto_session_id,
            }))
        }
        result => result,
    }
}

pub async fn append_pi_session_binding(
    connection: &mut SqliteConnection,
    oqto_session_id: &str,
    external_id: Option<&str>,
    source: &str,
) -> Result<Option<SessionBinding>, SessionBindingError> {
    let Some(external_id) = external_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    // Compatibility rows predating ADR-0023 may have no public identity yet.
    // Keep their legacy column readable, but never immortalize the raw harness
    // id as a new append-only Oqto identity fact.
    if !oqto_session_id.starts_with("oqto-") {
        return Ok(None);
    }
    let key = SessionBindingKey::pi_session(external_id);
    let binding_id = stable_binding_id(key, oqto_session_id, None);
    append_session_binding(
        connection,
        NewSessionBinding {
            binding_id: &binding_id,
            oqto_session_id,
            key,
            source,
            first_seen_at: None,
            last_seen_at: None,
            supersedes_binding_id: None,
            extensions_json: "{}",
        },
    )
    .await
    .map(Some)
}

pub fn stable_binding_id(
    key: SessionBindingKey<'_>,
    oqto_session_id: &str,
    supersedes_binding_id: Option<&str>,
) -> String {
    const NS: uuid::Uuid = uuid::uuid!("24998ba6-639d-4c20-89ea-e2d6adfdbac8");
    let input = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        key.harness,
        key.provider_instance,
        key.binding_kind,
        key.external_id,
        oqto_session_id,
        supersedes_binding_id.unwrap_or("")
    );
    format!("binding:{}", uuid::Uuid::new_v5(&NS, input.as_bytes()))
}
