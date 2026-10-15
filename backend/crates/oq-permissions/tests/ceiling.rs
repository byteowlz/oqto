#![cfg(unix)]

use oq_permissions::*;
use serde_json::json;
use std::path::PathBuf;
use tempfile::TempDir;

const HOME: &str = "/home/tester";
const WORKSPACE: &str = "/home/tester/Code/project";

/// Parse a `[guard]` body with oqto-sandbox's own sandbox.toml parser.
fn guard(body: &str) -> anyhow::Result<oqto_sandbox::GuardConfig> {
    let file: oqto_sandbox::SandboxConfigFile = toml::from_str(&format!("[guard]\n{body}"))?;
    Ok(file.guard.unwrap_or_default())
}
fn try_compile(global: Option<&str>, workspace: Option<&str>) -> anyhow::Result<Policy> {
    let mut input = input(None, None);
    input.owner = global.map(guard).transpose()?;
    input.workspace_guard = workspace.map(guard).transpose()?;
    compile_ceiling(&input)
}
fn input(global: Option<&str>, workspace: Option<&str>) -> CeilingInput {
    CeilingInput {
        owner: global.map(|body| guard(body).unwrap()),
        workspace_guard: workspace.map(|body| guard(body).unwrap()),
        home: PathBuf::from(HOME),
        workspace: PathBuf::from(WORKSPACE),
        hard_deny: vec![
            PathBuf::from("/var/tmp/runtime"),
            PathBuf::from("/home/tester/.pi/agent"),
        ],
        remembered: Remembered::default(),
    }
}
fn exec(resource: &str) -> Target {
    Target {
        capability: Capability::Execute,
        resource_id: resource.into(),
    }
}
fn path(capability: Capability, path: &str) -> Target {
    Target {
        capability,
        resource_id: format!("path:{path}"),
    }
}
fn read(path_: &str) -> Target {
    path(Capability::FileRead, path_)
}
fn tools() -> Target {
    exec(WORKSPACE_WRITE)
}
fn network() -> Target {
    Target {
        capability: Capability::Network,
        resource_id: NETWORK_EGRESS.into(),
    }
}

#[test]
fn defaults_ask_outside_and_hard_deny_secrets_roots_and_containers() {
    let p = compile_ceiling(&input(None, None)).unwrap();
    assert_eq!(
        p.mode(&exec(WORKSPACE_READ)),
        RuleMode::Allow,
        "reads are seamless"
    );
    assert_eq!(p.mode(&exec(WORKSPACE_WRITE)), RuleMode::Ask);
    assert_eq!(p.mode(&exec(WORKSPACE_SHELL)), RuleMode::Ask);
    assert_eq!(
        p.mode(&exec("shell:npm test")),
        RuleMode::Ask,
        "inherits shell"
    );
    assert_eq!(
        p.mode(&exec("shell:rm -rf;x")),
        RuleMode::Deny,
        "malformed prefix"
    );
    assert_eq!(p.mode(&network()), RuleMode::Ask);
    assert_eq!(p.mode(&read("/home/tester/Code/library")), RuleMode::Ask);
    assert_eq!(
        p.mode(&path(Capability::FileWrite, "/home/tester/Code/library")),
        RuleMode::Ask
    );
    for denied in [
        "/home/tester/.ssh",
        "/home/tester/.ssh/id_ed25519",
        "/home/tester/.config/gh",
        "/home/tester/.pi/agent/auth.json",
        "/home/tester", // contains secrets
        "/home",
        "/",
        "/etc/shadow",
        "/proc/self/environ",
        "/var/tmp/runtime/ledger",
        "/var/tmp",                      // contains a protected root
        "/home/tester/Code/project/src", // already granted to hands
        "/home/tester/Code",             // contains the workspace
        "/home/tester/Code/../.ssh",     // unnormalized never matches
        "relative/path",
    ] {
        assert_eq!(p.mode(&read(denied)), RuleMode::Deny, "{denied}");
    }
    // Unknown capabilities/resources stay denied.
    assert_eq!(
        p.mode(&Target {
            capability: Capability::Network,
            resource_id: "other".into()
        }),
        RuleMode::Deny
    );
}

#[test]
fn global_ceiling_shapes_requestable_paths() {
    let p = compile_ceiling(&input(
        Some(
            r#"
edit = "auto"
network = "deny"
outside = "deny"
paths = ["~/Code", "/opt/data"]
policy = { "~/Code/secrets/**" = "deny" }
"#,
        ),
        None,
    ))
    .unwrap();
    assert_eq!(p.mode(&tools()), RuleMode::Allow);
    assert_eq!(p.mode(&network()), RuleMode::Deny);
    assert_eq!(p.mode(&read("/home/tester/Code/library")), RuleMode::Ask);
    assert_eq!(p.mode(&read("/opt/data/set")), RuleMode::Ask);
    assert_eq!(p.mode(&read("/home/tester/Documents")), RuleMode::Deny);
    assert_eq!(
        p.mode(&read("/home/tester/Code/secrets/key")),
        RuleMode::Deny
    );
}

#[test]
fn workspace_file_can_only_narrow() {
    let global = r#"
edit = "prompt"
outside = "deny"
paths = ["~/Code"]
"#;
    // Attempts to pre-approve from the workspace fail closed.
    assert!(try_compile(Some(global), Some("edit = \"auto\"")).is_err());
    assert!(try_compile(Some(global), Some("network = \"auto\"")).is_err());
    // Asking for more from the workspace is ineffective.
    let widen = compile_ceiling(&input(
        Some(global),
        Some(
            r#"
outside = "prompt"
paths = ["~/Documents"]
"#,
        ),
    ))
    .unwrap();
    assert_eq!(widen.mode(&tools()), RuleMode::Ask);
    assert_eq!(widen.mode(&network()), RuleMode::Ask);
    assert_eq!(
        widen.mode(&read("/home/tester/Documents/x")),
        RuleMode::Deny
    );
    // Narrowing applies.
    let narrow = compile_ceiling(&input(
        Some(global),
        Some(
            r#"
edit = "deny"
network = "deny"
policy = { "../shared-secrets" = "deny" }
"#,
        ),
    ));
    let narrow = narrow.unwrap_err().to_string();
    assert!(narrow.contains(".."), "{narrow}");
    let narrow = compile_ceiling(&input(
        Some(global),
        Some(
            r#"
edit = "deny"
network = "deny"
outside = "deny"
policy = { "~/Code/library/private" = "deny" }
"#,
        ),
    ))
    .unwrap();
    assert_eq!(narrow.mode(&tools()), RuleMode::Deny);
    assert_eq!(narrow.mode(&network()), RuleMode::Deny);
    // Workspace layer has outside_paths=deny and no ask roots of its own.
    assert_eq!(
        narrow.mode(&read("/home/tester/Code/library")),
        RuleMode::Deny
    );
}

#[test]
fn unsupported_ceiling_values_fail_closed() {
    for bad in [
        "outside = \"auto\"",
        "unknown_key = 1",
        "edit = \"sometimes\"",
        "policy = { \"\" = \"deny\" }",
        "policy = { \"~/Code/*.rs\" = \"deny\" }",
        "this is not toml",
    ] {
        assert!(try_compile(Some(bad), None).is_err(), "{bad}");
        assert!(try_compile(None, Some(bad)).is_err(), "{bad}");
    }
}

#[test]
fn global_allow_paths_preapprove_without_prompts_and_lose_only_to_denies() {
    let p = compile_ceiling(&input(
        Some("policy = { \"~/Code/library\" = \"auto\" }"),
        None,
    ))
    .unwrap();
    assert_eq!(
        p.mode(&read("/home/tester/Code/library/data")),
        RuleMode::Allow
    );
    // A workspace can still deny, but cannot re-ask or narrow an explicit allow.
    let narrowed = compile_ceiling(&input(
        Some("policy = { \"~/Code/library/**\" = \"auto\" }"),
        Some("outside = \"deny\"\npolicy = { \"~/Code/library/private\" = \"deny\" }"),
    ))
    .unwrap();
    assert_eq!(
        narrowed.mode(&read("/home/tester/Code/library/data")),
        RuleMode::Allow
    );
    assert_eq!(
        narrowed.mode(&read("/home/tester/Code/library/private/key")),
        RuleMode::Deny
    );
    // The workspace file can never add allows.
    let err = try_compile(None, Some("policy = { \"/opt/data\" = \"auto\" }"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("owner-only"), "{err}");
    // Allowed and ask overlap in one layer is rejected.
    let err = try_compile(
        Some("paths = [\"/a\"]\npolicy = { \"/a\" = \"auto\" }"),
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("allowed and ask"), "{err}");
}

fn binding() -> Binding {
    Binding {
        account_id: "owner".into(),
        principal_id: "principal".into(),
        workspace_id: "workspace".into(),
        work_directory_id: "workdir".into(),
        session_id: "public-session".into(),
        audience: "owner-only-v1".into(),
    }
}
fn private_dir() -> TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}
fn op(id: &str, target: Target) -> Operation {
    Operation {
        request_id: id.into(),
        target,
        arguments: json!({"reason":"test"}),
    }
}
async fn approve(a: &Authority, operation: &Operation, lifetime: GrantLifetime, expires: u64) {
    let request = match a.admit(operation.clone(), 100).await.unwrap() {
        Admission::Pending(request) => *request,
        other => panic!("expected pending, got {other:?}"),
    };
    a.decide(
        "owner",
        &operation.request_id,
        &request.digest,
        request.revision,
        lifetime,
        true,
        expires,
        101,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn resource_grants_are_listed_until_revoked_or_expired() {
    let dir = private_dir();
    let policy = compile_ceiling(&input(None, None)).unwrap();
    let a = Authority::open(&dir.path().join("p.sqlite"), binding(), policy)
        .await
        .unwrap();
    let library = read("/home/tester/Code/library");
    approve(
        &a,
        &op("s:store:1", library.clone()),
        GrantLifetime::Session,
        5000,
    )
    .await;
    approve(
        &a,
        &op("s:store:2", network()),
        GrantLifetime::Session,
        2000,
    )
    .await;
    // Once grants bind one logical invocation, never an executor mount.
    approve(
        &a,
        &op("s:store:3", read("/home/tester/Code/other")),
        GrantLifetime::Once,
        5000,
    )
    .await;
    let mut active: Vec<_> = a
        .active_resource_grants(1000)
        .await
        .unwrap()
        .into_iter()
        .map(|(target, _)| target)
        .collect();
    active.sort_by_key(|t| t.resource_id.clone());
    let mut expected = vec![library.clone(), network()];
    expected.sort_by_key(|t| t.resource_id.clone());
    assert_eq!(active, expected);
    assert_eq!(
        a.active_resource_grants(3000).await.unwrap().len(),
        1,
        "expired network grant dropped"
    );
    a.revoke_all("owner").await.unwrap();
    assert!(a.active_resource_grants(1000).await.unwrap().is_empty());
}

#[tokio::test]
async fn changed_ceiling_needs_explicit_owner_replacement_which_revokes_everything() {
    let dir = private_dir();
    let file = dir.path().join("p.sqlite");
    let first = compile_ceiling(&input(None, None)).unwrap();
    let a = Authority::open(&file, binding(), first).await.unwrap();
    approve(
        &a,
        &op("s:store:1", read("/home/tester/Code/library")),
        GrantLifetime::Session,
        5000,
    )
    .await;
    let pending = op("s:store:2", read("/home/tester/Code/other"));
    assert!(matches!(
        a.admit(pending.clone(), 100).await.unwrap(),
        Admission::Pending(_)
    ));
    a.close().await;
    let changed = compile_ceiling(&input(Some("network = \"deny\""), None)).unwrap();
    assert!(
        Authority::open(&file, binding(), changed.clone())
            .await
            .is_err()
    );
    let mut other = binding();
    other.audience = "wider-audience".into();
    assert!(
        Authority::open_replacing_policy(&file, other, changed.clone())
            .await
            .is_err()
    );
    let b = Authority::open_replacing_policy(&file, binding(), changed.clone())
        .await
        .unwrap();
    assert!(
        b.active_resource_grants(1000).await.unwrap().is_empty(),
        "grants revoked"
    );
    // The discarded pending request is re-reviewed under the new ceiling.
    assert!(matches!(
        b.admit(pending, 200).await.unwrap(),
        Admission::Pending(_)
    ));
    b.close().await;
    // Replacement is idempotent; strict reopen now matches.
    Authority::open(&file, binding(), changed).await.unwrap();
}

#[test]
fn remembered_and_owner_rules_lift_ask_but_never_deny() {
    let mut remembering = input(Some("auto_shell = [\"cargo test\"]"), None);
    remembering.remembered = Remembered {
        read: false,
        edit: true,
        shell: false,
        shell_prefixes: vec!["npm test".into()],
    };
    let p = compile_ceiling(&remembering).unwrap();
    assert_eq!(p.mode(&exec(WORKSPACE_WRITE)), RuleMode::Allow);
    assert_eq!(p.mode(&exec("shell:npm test")), RuleMode::Allow);
    assert_eq!(p.mode(&exec("shell:cargo test")), RuleMode::Allow);
    assert_eq!(p.mode(&exec("shell:npm publish")), RuleMode::Ask);
    assert_eq!(p.mode(&exec(WORKSPACE_SHELL)), RuleMode::Ask);
    // A workspace deny beats remembered and owner rules.
    let mut denied = input(
        Some("auto_shell = [\"cargo test\"]"),
        Some("edit = \"deny\"\nshell = \"deny\""),
    );
    denied.remembered = Remembered {
        read: true,
        edit: true,
        shell: true,
        shell_prefixes: vec!["npm test".into()],
    };
    let p = compile_ceiling(&denied).unwrap();
    assert_eq!(p.mode(&exec(WORKSPACE_WRITE)), RuleMode::Deny);
    assert_eq!(p.mode(&exec("shell:npm test")), RuleMode::Deny);
    assert_eq!(p.mode(&exec("shell:cargo test")), RuleMode::Deny);
    // Only the owner may pre-approve shell commands; prefixes are plain words.
    assert!(try_compile(None, Some("auto_shell = [\"npm test\"]")).is_err());
    assert!(try_compile(Some("auto_shell = [\"rm -rf / ; x\"]"), None).is_err());
}

#[tokio::test]
async fn generic_shell_session_grant_covers_prefixed_commands_but_not_vice_versa() {
    let dir = private_dir();
    let policy = compile_ceiling(&input(None, None)).unwrap();
    let a = Authority::open(&dir.path().join("p.sqlite"), binding(), policy)
        .await
        .unwrap();
    // A prefix grant covers only that prefix.
    approve(
        &a,
        &op("s:store:1", exec("shell:npm test")),
        GrantLifetime::Session,
        5000,
    )
    .await;
    assert!(matches!(
        a.admit(op("s:store:2", exec("shell:npm test")), 200)
            .await
            .unwrap(),
        Admission::Allowed(_)
    ));
    assert!(matches!(
        a.admit(op("s:store:3", exec("shell:git push")), 200)
            .await
            .unwrap(),
        Admission::Pending(_)
    ));
    assert!(matches!(
        a.admit(op("s:store:4", exec(WORKSPACE_SHELL)), 200)
            .await
            .unwrap(),
        Admission::Pending(_)
    ));
    // A generic shell grant covers every shell command.
    approve(
        &a,
        &op("s:store:5", exec(WORKSPACE_SHELL)),
        GrantLifetime::Session,
        5000,
    )
    .await;
    assert!(matches!(
        a.admit(op("s:store:6", exec("shell:git push")), 300)
            .await
            .unwrap(),
        Admission::Allowed(_)
    ));
}
