//! Metadata-only Account Connection contracts. Identity and credential selection
//! are supplied by authenticated host management, never by an App payload.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppConnectionSlot {
    pub slot: String,
    pub expected_origin: String,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ConnectionPolicy {
    pub host: String,
    pub port: u16,
    pub path_prefix: String,
    pub method: String,
    pub header: String,
    pub attachment_prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ConnectionCreateRequest {
    pub label: String,
    pub policy: ConnectionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ConnectionUpdateRequest {
    pub expected_revision: u32,
    pub label: String,
    pub policy: ConnectionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ConnectionRevisionRequest {
    pub expected_revision: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ConnectionMetadata {
    pub id: String,
    pub revision: u32,
    pub label: String,
    pub policy: ConnectionPolicy,
    pub revoked: bool,
    /// Credential rotation requires explicit rebind; no transparent rotation.
    pub rotation_requires_rebind: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct AppConnectionBindRequest {
    pub connection_id: String,
    pub connection_revision: u32,
    pub reviewed_content_digest: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_management_rejects_caller_identity_and_provider_selection() {
        let valid = serde_json::json!({"label":"Example","policy":{"host":"api.example.com","port":443,"path_prefix":"/v1/","method":"GET","header":"authorization","attachment_prefix":"Bearer "}});
        for forbidden in [
            "account_id",
            "principal",
            "provider_id",
            "credential_id",
            "secret",
        ] {
            let mut request = valid.clone();
            request[forbidden] = "untrusted".into();
            assert!(
                serde_json::from_value::<ConnectionCreateRequest>(request).is_err(),
                "{forbidden}"
            );
        }
        assert!(serde_json::from_value::<ConnectionCreateRequest>(valid).is_ok());
    }
    #[test]
    fn canonical_connection_consent_pins_semantic_slot_and_destination() {
        let capability = crate::apps::AppCapabilityRequest::Connections {
            slots: vec![AppConnectionSlot {
                slot: "example".into(),
                expected_origin: "https://api.example.com".into(),
                profile: "https_get_v1".into(),
            }],
        };
        let json = serde_json::to_value(&capability).expect("canonical capability");
        assert_eq!(json["capability"], "connections");
        assert_eq!(
            json["slots"][0]["expected_origin"],
            "https://api.example.com"
        );
        assert_eq!(
            serde_json::from_value::<crate::apps::AppCapabilityRequest>(json).expect("round trip"),
            capability
        );
    }
}

/// Trusted host-to-runner credential identity, not an App-supplied credential selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialDescriptor {
    pub provider_id: String,
    pub credential_id: String,
    pub revision: String,
}
/// Host-authorized, single-dispatch intent. Contains no credential bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialHttpDispatch {
    pub account_id: String,
    pub workspace_id: String,
    pub credential: CredentialDescriptor,
    pub connection_id: String,
    pub connection_revision: u32,
    pub instance_id: String,
    pub content_digest: String,
    pub slot: String,
    pub audit_event_id: String,
    pub path: String,
    pub policy: ConnectionPolicy,
}
/// Workloads may select only a manifest-declared slot and relative operation;
/// Account, runner, credential, destination and grants come from host authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConnectionInvokeRequest {
    pub workspace_path: String,
    pub slot: String,
    pub path: String,
}

#[cfg(test)]
mod dispatch_contract_tests {
    use super::*;
    #[test]
    fn workload_cannot_supply_host_authority() {
        let base = serde_json::json!({"workspace_path":"/work","slot":"api","path":"/v1/list"});
        assert!(serde_json::from_value::<AppConnectionInvokeRequest>(base.clone()).is_ok());
        for field in [
            "account_id",
            "credential_id",
            "provider_id",
            "policy",
            "host",
            "header",
            "audit_event_id",
            "runner",
        ] {
            let mut value = base.clone();
            value[field] = "forged".into();
            assert!(
                serde_json::from_value::<AppConnectionInvokeRequest>(value).is_err(),
                "{field}"
            );
        }
    }
}
