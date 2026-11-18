use super::*;
use oqto_protocol::session_announce::{AnnounceReason, AnnounceVersion};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::time::{Duration, Instant};

struct Fixture {
    _temp: tempfile::TempDir,
    home: PathBuf,
    source: Arc<crate::personal_source::PersonalSource>,
    workspace: PathBuf,
    sockets: PathBuf,
    session_root: PathBuf,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let workspace = home.join("work/app");
    let session_root = home.join(".pi/agent/sessions");
    let sockets = home.join("run");
    std::fs::create_dir_all(&workspace).expect("workspace");
    std::fs::create_dir_all(&session_root).expect("session root");
    std::fs::create_dir_all(&sockets).expect("sockets");
    // macOS temp dirs are symlinks; the source contract requires canonical paths.
    let home = std::fs::canonicalize(&home).expect("canonical home");
    let workspace = std::fs::canonicalize(&workspace).expect("canonical workspace");
    let session_root = std::fs::canonicalize(&session_root).expect("canonical session root");
    let sockets = std::fs::canonicalize(&sockets).expect("canonical sockets");
    let source = Arc::new(
        crate::personal_source::PersonalSource::new(
            home.clone(),
            session_root.clone(),
            sockets.clone(),
            vec![workspace.clone()],
        )
        .expect("personal source"),
    );
    Fixture {
        _temp: temp,
        home,
        source,
        workspace,
        sockets,
        session_root,
    }
}

fn write_session(fixture: &Fixture, id: &str) -> PathBuf {
    let directory = fixture
        .session_root
        .join(oqto_pi::session_files::cwd_to_safe_dirname(
            &fixture.workspace,
        ));
    std::fs::create_dir_all(&directory).expect("session directory");
    let path = directory.join(format!("2026-11-05T05-40-30-000Z_{id}.jsonl"));
    let header = serde_json::json!({
        "type": "session",
        "id": id,
        "cwd": fixture.workspace,
    });
    std::fs::write(&path, format!("{header}\n")).expect("session header");
    path
}

fn announce(fixture: &Fixture, id: &str, path: &Path) -> SessionAnnounce {
    SessionAnnounce {
        v: AnnounceVersion::V1,
        kind: oqto_protocol::session_announce::SESSION_ANNOUNCE_KIND.to_owned(),
        reason: AnnounceReason::Start,
        session_file: path.display().to_string(),
        session_id: id.to_owned(),
        cwd: fixture.workspace.display().to_string(),
        pid: std::process::id(),
        live_socket: None,
        previous_session_file: None,
    }
}

async fn send(socket: &Path, announce: &SessionAnnounce) -> SessionAnnounceAck {
    let mut stream = UnixStream::connect(socket).await.expect("connect announce");
    let line = oqto_protocol::session_announce::encode_session_announce(announce).expect("encode");
    stream.write_all(line.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut raw))
        .await
        .expect("ack deadline")
        .expect("read ack");
    oqto_protocol::session_announce::decode_session_announce_ack(&raw).expect("ack decodes")
}

async fn send_raw(socket: &Path, raw: &[u8]) -> SessionAnnounceAck {
    let mut stream = UnixStream::connect(socket).await.expect("connect announce");
    stream.write_all(raw).await.expect("write");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .expect("ack deadline")
        .expect("read ack");
    oqto_protocol::session_announce::decode_session_announce_ack(&response).expect("ack decodes")
}

fn listener(fixture: &Fixture) -> AnnounceListener {
    spawn(
        Arc::clone(&fixture.source),
        fixture.home.clone(),
        "synthetic".into(),
    )
    .expect("announce listener")
}

#[tokio::test]
async fn announce_lists_a_new_native_session_within_two_seconds() {
    let fixture = fixture();
    let listener = listener(&fixture);
    let path = write_session(&fixture, "ses_instant");
    assert!(
        !oqto_history::oqto_log::ops::native_source::is_bound(&fixture.home, "ses_instant")
            .await
            .expect("bound probe")
    );
    let started = Instant::now();
    let ack = send(&listener.socket, &announce(&fixture, "ses_instant", &path)).await;
    assert_eq!(ack.result, AnnounceOutcome::Listed, "{ack:?}");
    assert!(ack.ok);
    assert!(
        oqto_history::oqto_log::ops::native_source::is_bound(&fixture.home, "ses_instant")
            .await
            .expect("bound probe")
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "listed after {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_second_announce_for_the_same_file_is_a_duplicate_not_a_second_row() {
    let fixture = fixture();
    let listener = listener(&fixture);
    let path = write_session(&fixture, "ses_twice");
    let first = send(&listener.socket, &announce(&fixture, "ses_twice", &path)).await;
    let second = send(&listener.socket, &announce(&fixture, "ses_twice", &path)).await;
    assert_eq!(first.result, AnnounceOutcome::Listed);
    assert_eq!(second.result, AnnounceOutcome::Duplicate);
}

/// Control for the test above: without the announce listener nothing lists that
/// file within the same window, which is exactly the 5-minute-sweep behaviour
/// this task removes.
#[tokio::test]
async fn sweep_only_does_not_list_within_two_seconds() {
    let fixture = fixture();
    let path = write_session(&fixture, "ses_sweep");
    assert!(path.exists());
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        !oqto_history::oqto_log::ops::native_source::is_bound(&fixture.home, "ses_sweep")
            .await
            .expect("bound probe")
    );
}

#[tokio::test]
async fn out_of_root_session_file_is_denied() {
    let fixture = fixture();
    let listener = listener(&fixture);
    let path = write_session(&fixture, "ses_outside");
    let outside = fixture.home.join("elsewhere_ses_outside.jsonl");
    std::fs::create_dir_all(outside.parent().expect("parent")).expect("elsewhere");
    std::fs::copy(&path, &outside).expect("copy");
    let mut forged = announce(&fixture, "ses_outside", &outside);
    forged.reason = AnnounceReason::New;
    let ack = send(&listener.socket, &forged).await;
    assert_eq!(ack.result, AnnounceOutcome::Denied, "{ack:?}");
    assert!(!ack.ok);
    assert!(
        !oqto_history::oqto_log::ops::native_source::is_bound(&fixture.home, "ses_outside")
            .await
            .expect("bound probe")
    );
}

#[tokio::test]
async fn workspace_outside_approved_roots_is_denied() {
    let fixture = fixture();
    let listener = listener(&fixture);
    let path = write_session(&fixture, "ses_other_root");
    let mut forged = announce(&fixture, "ses_other_root", &path);
    forged.cwd = fixture.home.join("other").display().to_string();
    let ack = send(&listener.socket, &forged).await;
    assert_eq!(ack.result, AnnounceOutcome::Denied, "{ack:?}");
}

#[tokio::test]
async fn identity_and_live_socket_must_match_the_source() {
    let fixture = fixture();
    let listener = listener(&fixture);
    let path = write_session(&fixture, "ses_ident");
    let mut forged = announce(&fixture, "ses_ident", &path);
    forged.session_id = "ses_somebody_else".to_owned();
    let ack = send(&listener.socket, &forged).await;
    assert_eq!(ack.result, AnnounceOutcome::Denied, "{ack:?}");

    let mut socket_outside = announce(&fixture, "ses_ident", &path);
    socket_outside.live_socket = Some("/tmp/pi-tui-rpc-1.sock".to_owned());
    let ack = send(&listener.socket, &socket_outside).await;
    assert_eq!(ack.result, AnnounceOutcome::Denied, "{ack:?}");
    assert!(
        !oqto_history::oqto_log::ops::native_source::is_bound(&fixture.home, "ses_ident")
            .await
            .expect("bound probe")
    );
}

#[tokio::test]
async fn a_symlinked_session_file_is_denied() {
    let fixture = fixture();
    let listener = listener(&fixture);
    let path = write_session(&fixture, "ses_link");
    let link = path.with_file_name("2026-11-05T05-41-30-000Z_ses_link.jsonl");
    std::os::unix::fs::symlink(&path, &link).expect("symlink");
    let ack = send(&listener.socket, &announce(&fixture, "ses_link", &link)).await;
    assert_eq!(ack.result, AnnounceOutcome::Denied, "{ack:?}");
}

#[tokio::test]
async fn invalid_announces_are_refused_before_any_file_is_read() {
    let fixture = fixture();
    let listener = listener(&fixture);
    let ack = send_raw(&listener.socket, b"not an announce\n").await;
    assert_eq!(ack.result, AnnounceOutcome::Invalid, "{ack:?}");
    let ack = send_raw(
        &listener.socket,
        b"{\"v\":2,\"kind\":\"session_announce\",\"reason\":\"start\",\"session_file\":\"/x.jsonl\",\"session_id\":\"ses\",\"cwd\":\"/x\",\"pid\":1}\n",
    )
    .await;
    assert_eq!(ack.result, AnnounceOutcome::Invalid, "{ack:?}");
    assert_eq!(ack.detail, "wrong_version");
}

#[tokio::test]
async fn the_listener_lives_in_the_private_socket_root_only() {
    let fixture = fixture();
    let listener = listener(&fixture);
    assert_eq!(listener.socket, fixture.sockets.join(SOCKET_NAME));
    let meta = std::fs::symlink_metadata(&listener.socket).expect("socket");
    assert!(meta.file_type().is_socket());
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
}

#[test]
fn a_peer_from_another_uid_is_refused() {
    // The socket is never authentication: the same check guards every connection.
    let ours = rustix::process::geteuid().as_raw();
    assert!(super::peer_allowed(ours, ours));
    assert!(!super::peer_allowed(ours, ours + 1));
}
