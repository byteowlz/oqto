//! Durable Account-owned Connection policy and App slot binding authority.
//! No credential bytes, transport execution, or independent App grants live here.
mod audit;
pub use audit::RunnerConnectionAudit;
mod policy;
mod repository;
mod service;
use oqto_protocol::credential_connections::AppConnectionSlot;
pub use repository::ConnectionRepository;
pub use service::{ConnectionAudit, ConnectionAuditEvent, ConnectionInvocation, ConnectionService};

/// Host-authenticated provider metadata; never accepted from public App DTOs.
#[derive(Clone, PartialEq, Eq)]
pub struct CredentialReference {
    pub provider_id: String,
    pub credential_id: String,
    pub revision: String,
}
impl std::fmt::Debug for CredentialReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialReference(<host-owned metadata>)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveConnectionSlot {
    pub definition_id: String,
    pub content_digest: String,
    pub request: AppConnectionSlot,
    pub(crate) canonical_json: String,
    pub(crate) grant_json: String,
}

pub(crate) const LIVE_APP_AUTHORITY_SQL: &str = r#"
SELECT d.id, d.content_digest, d.requested_capabilities_json, g.request_json
FROM app_instances i
JOIN app_definitions d ON d.id=i.definition_id
JOIN app_installations installation ON installation.id=i.installation_id
JOIN work_directories w ON w.id=i.binding_id
JOIN app_capability_grants g ON g.instance_id=i.id
WHERE i.id=? AND i.created_by_account_id=? AND i.binding_kind='work_directory'
AND i.binding_id=? AND w.owner_kind=? AND w.owner_id=? AND w.binding_path=?
AND i.status='active' AND installation.owner_kind='work_directory'
AND installation.owner_id=w.id AND installation.current_definition_id=d.id
AND g.definition_id=d.id AND g.content_digest=d.content_digest
AND g.decided_by_account_id=i.created_by_account_id
AND g.decision='allowed' AND g.revoked_at IS NULL
"#;

#[cfg(test)]
mod tests;
