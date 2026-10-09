//! Non-chat extension controls. Metadata is routing information, never a grant.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

pub const CONTROL_MAX_BYTES: usize = 256 * 1024;

/// Closed vocabulary; arbitrary slash commands cannot enter this path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub enum ControlOperation {
    PresentationBind,
    PresentationList,
    PresentationUnbind,
    SelectionCreate,
    SelectionGet,
    SelectionOpen,
    SelectionClose,
    SelectionSave,
    SelectionSubmit,
    SelectionAck,
}

impl ControlOperation {
    pub fn command(self) -> &'static str {
        match self {
            Self::PresentationBind => "presentation-bind",
            Self::PresentationList => "presentation-list",
            Self::PresentationUnbind => "presentation-unbind",
            Self::SelectionCreate => "selection-create",
            Self::SelectionGet => "selection-get",
            Self::SelectionOpen => "selection-open",
            Self::SelectionClose => "selection-close",
            Self::SelectionSave => "selection-save",
            Self::SelectionSubmit => "selection-submit",
            Self::SelectionAck => "selection-ack",
        }
    }

    pub fn required_commands(self) -> &'static [&'static str] {
        match self {
            Self::PresentationBind | Self::PresentationList | Self::PresentationUnbind => &[
                "presentation-bind",
                "presentation-list",
                "presentation-unbind",
            ],
            Self::SelectionCreate => &["selection-create"],
            Self::SelectionGet => &["selection-get"],
            Self::SelectionOpen => &["selection-open"],
            Self::SelectionClose => &["selection-close"],
            Self::SelectionSave => &["selection-save"],
            Self::SelectionSubmit => &["selection-submit"],
            Self::SelectionAck => &["selection-ack"],
        }
    }

    pub fn status_key(self) -> &'static str {
        match self {
            Self::PresentationBind | Self::PresentationList | Self::PresentationUnbind => {
                "pi-capabilities:reply/v1"
            }
            _ => "pi-selection:reply/v1",
        }
    }

    /// Transport-owned fields are never taken from an untrusted caller.
    pub fn arguments(
        self,
        arguments: Value,
        request_id: &str,
        binding_id: &str,
    ) -> Result<Map<String, Value>, String> {
        if serde_json::to_vec(&arguments)
            .map_err(|e| e.to_string())?
            .len()
            > CONTROL_MAX_BYTES
        {
            return Err("Control arguments exceed limit".into());
        }
        let Value::Object(mut args) = arguments else {
            return Err("Control arguments must be an object".into());
        };
        let fields: &[&str] = match self {
            Self::PresentationBind => &["clientKind", "capabilities", "leaseMs"],
            Self::PresentationList | Self::PresentationUnbind => &[],
            Self::SelectionCreate => &["spec", "presentation"],
            Self::SelectionGet | Self::SelectionAck => &["id"],
            Self::SelectionClose => &["id", "revision"],
            Self::SelectionOpen => &["id", "presentation"],
            Self::SelectionSave | Self::SelectionSubmit => &["id", "revision", "answers"],
        };
        if args.keys().any(|key| !fields.contains(&key.as_str())) {
            return Err("Unknown or transport-owned control field".into());
        }
        if self == Self::SelectionClose
            && !args
                .get("revision")
                .and_then(Value::as_u64)
                .is_some_and(|revision| revision <= 9_007_199_254_740_991)
        {
            return Err("Selection close requires an observed safe-integer revision".into());
        }
        args.insert("version".into(), Value::from(1));
        args.insert("requestId".into(), Value::from(request_id));
        match self {
            Self::PresentationBind | Self::PresentationUnbind => {
                args.insert("id".into(), Value::from(binding_id));
            }
            Self::SelectionAck | Self::SelectionCreate | Self::SelectionOpen => {
                args.insert("bindingId".into(), Value::from(binding_id));
            }
            _ => {}
        }
        Ok(args)
    }
}

/// Non-chat payload; authentication and connection identity belong to the transport.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    pub operation: ControlOperation,
    pub arguments: Value,
}

/// Bounded authoritative acknowledgement; the outer envelope retains platform identity.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ControlResult {
    /// Gateway-owned current connection/Session binding; never infer from a snapshot.
    pub binding_id: String,
    pub request_id: String,
    pub operation: ControlOperation,
    pub native_session_id: String,
    pub reply: Value,
}

pub fn is_control_status(key: &str) -> bool {
    matches!(
        key,
        "pi-capabilities:reply/v1" | "pi-selection:reply/v1" | "pi-selection:present/v1"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn close_requires_and_preserves_the_observed_nonnegative_integer_revision() {
        for revision in [
            json!(null),
            json!(-1),
            json!(1.5),
            json!("1"),
            json!(9_007_199_254_740_992_u64),
        ] {
            assert!(
                ControlOperation::SelectionClose
                    .arguments(
                        json!({"id":"review", "revision":revision}),
                        "request",
                        "connection"
                    )
                    .is_err()
            );
        }
        assert!(
            ControlOperation::SelectionClose
                .arguments(json!({"id":"review"}), "request", "connection")
                .is_err()
        );
        for revision in [0_u64, 1, 9_007_199_254_740_991] {
            let args = ControlOperation::SelectionClose
                .arguments(
                    json!({"id":"review", "revision":revision}),
                    "request",
                    "connection",
                )
                .expect("observed CAS revision");
            assert_eq!(args["revision"], revision);
            assert_eq!(args["id"], "review");
        }
    }

    #[test]
    fn canonical_control_shape_is_flat_strict_and_has_no_prompt_client_id() {
        let value = json!({"session_id":"oqto-platform", "id":"request", "cmd":"extension_control", "operation":"selection_get", "arguments":{"id":"review"}});
        let command: crate::commands::Command =
            serde_json::from_value(value.clone()).expect("canonical command");
        assert_eq!(serde_json::to_value(command).expect("serialize"), value);
        for field in ["client_id", "native_session_id", "binding_id"] {
            let mut forged = value.clone();
            forged[field] = json!("forged");
            assert!(serde_json::from_value::<crate::commands::Command>(forged).is_err());
        }
    }

    #[test]
    fn closed_vocabulary_and_connection_owned_identity() {
        assert!(serde_json::from_value::<ControlOperation>(json!("reload")).is_err());
        for field in [
            "id",
            "scopeId",
            "requestId",
            "bindingId",
            "version",
            "endpoint",
        ] {
            assert!(
                ControlOperation::PresentationBind
                    .arguments(json!({field: "forged"}), "req", "connection")
                    .is_err()
            );
        }
        let args = ControlOperation::SelectionAck
            .arguments(json!({"id":"review"}), "req", "connection")
            .expect("valid");
        assert_eq!(args["bindingId"], "connection");
        assert_eq!(args["id"], "review");
        assert_eq!(args["requestId"], "req");
        assert!(
            ControlOperation::SelectionGet
                .arguments(
                    json!({"id":"x".repeat(CONTROL_MAX_BYTES)}),
                    "req",
                    "connection"
                )
                .is_err()
        );
    }
}
