//! Authenticated credential CONTROL transport only. Never put these requests in
//! history, Host Bridge messages, agent capabilities, tool arguments or logs.
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zeroize::Zeroizing;

/// Secret input serialized only on the protected authenticated control channel.
#[derive(Clone, Default)]
pub struct CredentialSecretInput(Zeroizing<String>);
impl CredentialSecretInput {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
    pub fn expose_for_protected_transport(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for CredentialSecretInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialSecretInput([REDACTED])")
    }
}
impl Serialize for CredentialSecretInput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for CredentialSecretInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
#[ts(tag = "action", rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum CredentialCommand {
    Status,
    Initialize {
        #[ts(type = "string")]
        passphrase: CredentialSecretInput,
    },
    Unlock {
        #[ts(type = "string")]
        passphrase: CredentialSecretInput,
    },
    Lock,
    List,
    Put {
        id: String,
        #[ts(type = "string")]
        value: CredentialSecretInput,
        #[ts(type = "number | null")]
        expected_revision: Option<u64>,
    },
    Delete {
        id: String,
        #[ts(type = "number")]
        expected_revision: u64,
    },
}
/// Account is derived by the authenticated host, never an agent-supplied identity.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct CredentialManageRequest {
    pub account_id: String,
    pub command: CredentialCommand,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum CredentialProviderStatus {
    Uninitialized,
    Locked,
    Ready,
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct CredentialEntryMetadata {
    pub id: String,
    #[ts(type = "number")]
    pub revision: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct CredentialManageResult {
    pub status: CredentialProviderStatus,
    pub entries: Vec<CredentialEntryMetadata>,
    pub code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_contract_matches_serde() {
        let declaration = CredentialCommand::decl();
        assert!(declaration.contains("action"));
        assert!(declaration.contains("initialize"));
        assert!(!declaration.contains("CredentialSecretInput"));
        assert!(CredentialProviderStatus::decl().contains("uninitialized"));
    }

    #[test]
    fn protected_transport_and_redacted_debug() -> Result<(), serde_json::Error> {
        let command = CredentialCommand::Unlock {
            passphrase: CredentialSecretInput::new("synthetic-passphrase".into()),
        };
        assert!(!format!("{command:?}").contains("synthetic-passphrase"));
        let encoded = serde_json::to_string(&command)?;
        assert_eq!(
            encoded,
            r#"{"action":"unlock","passphrase":"synthetic-passphrase"}"#
        );
        let decoded: CredentialCommand = serde_json::from_str(&encoded)?;
        assert!(!format!("{decoded:?}").contains("synthetic-passphrase"));
        Ok(())
    }
}
