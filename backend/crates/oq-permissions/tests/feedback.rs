use oq_permissions::*;
use serde_json::json;

fn policy() -> Policy {
    Policy {
        revision: "feedback-v1".into(),
        rules: vec![],
        paths: vec![
            PathLayer {
                builtin: true,
                default: RuleMode::Ask,
                deny: vec!["/home/owner/.private-credential".into()],
                ask: vec![],
                allow: vec![],
            },
            PathLayer {
                builtin: false,
                default: RuleMode::Ask,
                deny: vec!["/data/restricted".into()],
                ask: vec![],
                allow: vec![],
            },
        ],
    }
}
fn operation(id: &str, path: &str) -> Operation {
    Operation {
        request_id: id.into(),
        target: Target {
            capability: Capability::FileRead,
            resource_id: format!("path:{path}"),
        },
        arguments: json!({}),
    }
}
#[tokio::test]
async fn authoritative_safe_attribution_and_pending_expiry() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let authority = Authority::open(
        &dir.path().join("ledger"),
        Binding {
            account_id: "owner".into(),
            principal_id: "owner".into(),
            workspace_id: "workspace".into(),
            work_directory_id: "directory".into(),
            session_id: "session".into(),
            audience: "owner".into(),
        },
        policy(),
    )
    .await
    .unwrap();
    for (id, path, code) in [
        ("ancestor", "/home/owner", DenialCode::ProtectedSafety),
        (
            "protected",
            "/home/owner/.private-credential",
            DenialCode::ProtectedSafety,
        ),
        (
            "configured",
            "/data/restricted",
            DenialCode::ConfiguredCeiling,
        ),
    ] {
        let admission = authority.admit(operation(id, path), 100).await.unwrap();
        let encoded = serde_json::to_string(&admission).unwrap();
        assert!(!encoded.contains("/home"));
        assert!(!encoded.contains("credential"));
        assert!(!encoded.contains("/data"));
        let Admission::Denied(denial) = admission else {
            panic!("must be denied")
        };
        assert_eq!(denial.code, code);
    }
    let request = operation("expires", "/data/requestable");
    assert!(matches!(
        authority.admit(request.clone(), 100).await.unwrap(),
        Admission::Pending(_)
    ));
    assert!(matches!(
        authority.admit(request, 300_100).await.unwrap(),
        Admission::Denied(Denial {
            code: DenialCode::Expired
        })
    ));
    assert_eq!(
        policy().denial_code(&operation("bad", "relative").target),
        DenialCode::Unknown
    );
}
