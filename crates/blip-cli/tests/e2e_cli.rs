use assert_cmd::Command;
use blip_core::{BlipStore, LocalBlobStore, NewClipboardPayload, PayloadKind};
use predicates::prelude::*;
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use std::path::Path;
use std::process::{Child, Command as ProcessCommand};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

fn blip_command(db_path: &Path) -> Command {
    let mut cmd = Command::cargo_bin("blip").expect("binary should build");
    cmd.env("BLIPCOARD_DB_PATH", db_path);
    if let Some(parent) = db_path.parent() {
        cmd.env("BLIPCOARD_CONFIG_DIR", parent.join("config"));
    }
    cmd
}

fn blip_command_with_socket(db_path: &Path, socket_path: &Path) -> Command {
    let mut cmd = blip_command(db_path);
    cmd.env("BLIPCOARD_SOCKET_PATH", socket_path);
    cmd
}

fn blip_daemon_command(db_path: &Path, socket_path: &Path) -> ProcessCommand {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = ProcessCommand::new(cargo);
    cmd.args(["run", "-q", "-p", "blip-daemon", "--", "--ipc-only"]);
    cmd.env("BLIPCOARD_DB_PATH", db_path);
    cmd.env("BLIPCOARD_SOCKET_PATH", socket_path);
    cmd
}

#[test]
fn cli_can_manage_workspace_and_blips_end_to_end() {
    let temp = tempdir().expect("tempdir should exist");
    let db_path = temp.path().join("blipcoard-test.db");

    blip_command(&db_path)
        .args(["create", "auth-bug", "--description", "Auth bug triage"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created workspace auth-bug"));

    blip_command(&db_path)
        .args(["create", "agent-feed", "--agent-access"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created workspace agent-feed"));

    blip_command(&db_path)
        .args([
            "add-demo",
            "auth-bug",
            "TypeError: broken login flow",
            "--source-app",
            "Firefox",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("created blip"));

    blip_command(&db_path)
        .args(["add-demo", "agent-feed", "Agent-visible deployment note"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created blip"));

    blip_command(&db_path)
        .args(["add-demo", "agent-feed", "Rollback deployment note"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created blip"));

    blip_command(&db_path)
        .args(["add-demo", "inbox", "Copied inbox note"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created blip"));

    let socket_path = temp.path().join("blipcoard.sock");
    let mut daemon = blip_daemon_command(&db_path, &socket_path)
        .spawn()
        .expect("daemon should start");
    wait_for_socket(&socket_path);

    blip_command_with_socket(&db_path, &socket_path)
        .args(["use", "auth-bug"])
        .assert()
        .success()
        .stdout(predicate::str::contains("active workspace set to auth-bug"));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["use", "missing"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "daemon returned not_found: workspace `missing` does not exist",
        ));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth-bug"));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["workspaces"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth-bug [human-only]"))
        .stdout(predicate::str::contains("agent-feed [agent-readable]"));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[type:plain_text]"))
        .stdout(predicate::str::contains("Copied inbox note"));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["list", "auth-bug"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TypeError: broken login flow"));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["search", "auth-bug", "login"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TypeError: broken login flow"));

    let auth_search = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["search", "auth-bug", "login", "--output", "json"],
    );
    assert_eq!(auth_search["workspace"], "auth-bug");
    assert!(
        auth_search["blips"]
            .as_array()
            .expect("search blips should be an array")
            .iter()
            .any(|blip| blip["preview"] == "TypeError: broken login flow")
    );

    let agent_recent = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["agent", "recent", "agent-feed", "--output", "json"],
    );
    assert_eq!(agent_recent["workspace"], "agent-feed");
    assert!(
        agent_recent["blips"]
            .as_array()
            .expect("agent blips should be an array")
            .iter()
            .any(|blip| blip["content"] == "Agent-visible deployment note")
    );

    let agent_search = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &[
            "agent",
            "search",
            "agent-feed",
            "rollback",
            "--output",
            "json",
        ],
    );
    assert_eq!(agent_search["workspace"], "agent-feed");
    assert!(
        agent_search["blips"]
            .as_array()
            .expect("agent search blips should be an array")
            .iter()
            .any(|blip| blip["content"] == "Rollback deployment note")
    );

    blip_command_with_socket(&db_path, &socket_path)
        .args(["agent", "bundle", "agent-feed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# blipcoard bundle"))
        .stdout(predicate::str::contains("workspace: agent-feed"))
        .stdout(predicate::str::contains("Rollback deployment note"));

    let agent_bundle = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["agent", "bundle", "agent-feed", "--output", "json"],
    );
    assert_eq!(agent_bundle["workspace"], "agent-feed");
    assert_eq!(agent_bundle["format"], "markdown");
    assert!(
        agent_bundle["content"]
            .as_str()
            .expect("bundle content should be a string")
            .contains("# blipcoard bundle")
    );

    blip_command_with_socket(&db_path, &socket_path)
        .args(["agent", "recent", "auth-bug"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "daemon returned access_denied: agent access to workspace `auth-bug` is denied",
        ));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["agent", "search", "auth-bug", "login"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "daemon returned access_denied: agent access to workspace `auth-bug` is denied",
        ));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["agent", "bundle", "auth-bug"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "daemon returned access_denied: agent access to workspace `auth-bug` is denied",
        ));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["agent", "recent", "inbox"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "daemon returned access_denied: agent access to workspace `inbox` is denied",
        ));

    let health = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["health", "--output", "json"],
    );
    assert_eq!(health["service"], "blipd");
    assert_eq!(health["status"], "ready");
    assert_eq!(health["active_workspace"], "auth-bug");

    let current = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["current", "--output", "json"],
    );
    assert_eq!(current["active_workspace"], "auth-bug");

    let workspaces = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["workspaces", "--output", "json"],
    );
    let workspaces = workspaces["workspaces"]
        .as_array()
        .expect("workspaces should be an array");
    assert!(
        workspaces
            .iter()
            .any(|workspace| workspace["name"] == "auth-bug" && workspace["agent_access"] == false)
    );
    assert!(
        workspaces
            .iter()
            .any(|workspace| workspace["name"] == "agent-feed" && workspace["agent_access"] == true)
    );

    let inbox = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["inbox", "--output", "json"],
    );
    assert_eq!(inbox["workspace"], "inbox");
    assert!(
        inbox["blips"]
            .as_array()
            .expect("inbox blips should be an array")
            .iter()
            .any(|blip| blip["preview"] == "Copied inbox note")
    );
    assert!(
        inbox["blips"]
            .as_array()
            .expect("inbox blips should be an array")
            .iter()
            .any(|blip| {
                blip["preview"] == "Copied inbox note"
                    && blip["tags"]
                        .as_array()
                        .expect("tags should be an array")
                        .iter()
                        .any(|tag| tag == "type:plain_text")
            })
    );

    let routed = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["send", "auth-bug", "--output", "json"],
    );
    let routed_id = routed["id"]
        .as_str()
        .expect("routed id should be a string")
        .to_owned();
    assert_eq!(routed["from_workspace"], "inbox");
    assert_eq!(routed["to_workspace"], "auth-bug");

    let auth_bug = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["list", "auth-bug", "--output", "json"],
    );
    assert_eq!(auth_bug["workspace"], "auth-bug");
    assert!(
        auth_bug["blips"]
            .as_array()
            .expect("auth-bug blips should be an array")
            .iter()
            .any(|blip| blip["preview"] == "TypeError: broken login flow")
    );
    assert!(
        auth_bug["blips"]
            .as_array()
            .expect("auth-bug blips should be an array")
            .iter()
            .any(|blip| blip["id"] == routed_id)
    );

    blip_command_with_socket(&db_path, &socket_path)
        .args(["send", "inbox", "--id", &routed_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "routed blip {routed_id} from auth-bug to inbox"
        )));

    let inbox_after_recovery = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["inbox", "--output", "json"],
    );
    assert!(
        inbox_after_recovery["blips"]
            .as_array()
            .expect("inbox blips should be an array")
            .iter()
            .any(|blip| blip["id"] == routed_id)
    );

    blip_command_with_socket(&db_path, &socket_path)
        .args(["send", "missing", "--id", &routed_id])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "daemon returned not_found: workspace `missing` does not exist",
        ));
    stop_daemon(&mut daemon);

    let agent_access = Connection::open(&db_path)
        .expect("database should open")
        .query_row(
            "SELECT agent_access FROM workspaces WHERE name = 'auth-bug'",
            [],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .expect("workspace query should succeed")
        .expect("workspace should exist");
    assert!(
        !agent_access,
        "workspace should default to human-only access"
    );
}

#[test]
fn cli_rejects_duplicate_workspace_creation() {
    let temp = tempdir().expect("tempdir should exist");
    let db_path = temp.path().join("blipcoard-test.db");

    blip_command(&db_path)
        .args(["create", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created workspace demo"));

    blip_command(&db_path)
        .args(["create", "demo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("workspace `demo` already exists"));
}

#[test]
fn cli_surfaces_secret_detection_in_list_output() {
    let temp = tempdir().expect("tempdir should exist");
    let db_path = temp.path().join("blipcoard-test.db");
    let socket_path = temp.path().join("blipcoard.sock");

    blip_command(&db_path)
        .args(["add-demo", "inbox", "api_key = abcdef1234567890"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created blip"));

    let mut daemon = blip_daemon_command(&db_path, &socket_path)
        .spawn()
        .expect("daemon should start");
    wait_for_socket(&socket_path);

    blip_command_with_socket(&db_path, &socket_path)
        .args(["inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[type:plain_text]"))
        .stdout(predicate::str::contains("[secret]"))
        .stdout(predicate::str::contains("api_key = abcdef1234567890"));

    let inbox = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["inbox", "--output", "json"],
    );
    let blip = inbox["blips"]
        .as_array()
        .expect("inbox blips should be an array")
        .first()
        .expect("secret blip should be present");
    assert_eq!(blip["is_redacted"], false);
    assert!(
        blip["tags"]
            .as_array()
            .expect("tags should be an array")
            .iter()
            .any(|tag| tag == "secret:assignment")
    );

    stop_daemon(&mut daemon);
}

#[test]
fn cli_summarizes_rich_payloads_without_binary_output() {
    let temp = tempdir().expect("tempdir should exist");
    let db_path = temp.path().join("blipcoard-test.db");
    let socket_path = temp.path().join("blipcoard.sock");

    blip_command(&db_path)
        .args(["add-demo", "inbox", "File-list clipboard payload: 2 paths"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created blip"));

    let connection = Connection::open(&db_path).expect("database should open");
    let blip_id = connection
        .query_row(
            "SELECT id FROM blips WHERE content = ?1",
            ["File-list clipboard payload: 2 paths"],
            |row| row.get::<_, String>(0),
        )
        .expect("seed blip should exist");
    connection
        .execute(
            "INSERT INTO blip_payloads (
                id, blip_id, payload_kind, mime_type, platform_format, byte_size,
                source_app, captured_at, preview_ref, blob_ref, inline_text,
                metadata_json, created_at
             ) VALUES (?1, ?2, 'file_list', 'text/uri-list', 'test:file-list', 42,
                NULL, ?3, NULL, NULL, NULL, ?4, ?3)",
            rusqlite::params![
                format!("{blip_id}:payload:file-list"),
                blip_id,
                "2026-07-03T12:00:00Z",
                serde_json::json!({
                    "policy": "metadata_only",
                    "path_count": 2,
                    "paths": [
                        { "display_path": "/tmp/a.txt" },
                        { "display_path": "/tmp/b.txt" }
                    ]
                })
                .to_string(),
            ],
        )
        .expect("file-list payload should insert");

    let mut daemon = blip_daemon_command(&db_path, &socket_path)
        .spawn()
        .expect("daemon should start");
    wait_for_socket(&socket_path);

    blip_command_with_socket(&db_path, &socket_path)
        .args(["inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "[payload:file_list,metadata_only,id=",
        ))
        .stdout(predicate::str::contains("mime=text/uri-list"))
        .stdout(predicate::str::contains("size=42B"))
        .stdout(predicate::str::contains(
            "File-list clipboard payload: 2 paths",
        ));

    let inbox = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["inbox", "--output", "json"],
    );
    let payload = inbox["blips"]
        .as_array()
        .expect("inbox blips should be an array")
        .iter()
        .find(|blip| blip["id"] == blip_id)
        .and_then(|blip| blip["payloads"].as_array())
        .and_then(|payloads| {
            payloads
                .iter()
                .find(|payload| payload["payload_kind"] == "file_list")
        })
        .expect("file-list payload summary should be present");
    assert_eq!(payload["preview_state"], "metadata_only");
    assert!(payload.get("blob_ref").is_none());

    stop_daemon(&mut daemon);
}

#[test]
fn cli_inspects_and_exports_rich_payload_bytes_safely() {
    let temp = tempdir().expect("tempdir should exist");
    let db_path = temp.path().join("blipcoard-test.db");
    let socket_path = temp.path().join("blipcoard.sock");

    blip_command(&db_path)
        .args(["create", "payload-lab", "--agent-access"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created workspace payload-lab"));

    blip_command(&db_path)
        .args(["add-demo", "payload-lab", "Image clipboard payload: test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("created blip"));

    let blip_id = Connection::open(&db_path)
        .expect("database should open")
        .query_row(
            "SELECT id FROM blips WHERE content = ?1",
            ["Image clipboard payload: test"],
            |row| row.get::<_, String>(0),
        )
        .expect("seed blip should exist");
    let mut store = BlipStore::open(&db_path).expect("store should open");
    let blob_store = LocalBlobStore::new(temp.path());
    let preview = blob_store
        .write(b"preview bytes")
        .expect("preview should write");
    let payload = store
        .insert_blob_payload(
            &blip_id,
            &NewClipboardPayload {
                kind: PayloadKind::Image,
                mime_type: Some("image/png".to_owned()),
                platform_format: Some("public.png".to_owned()),
                source_app: None,
                preview_ref: Some(preview.blob_ref),
                inline_text: None,
                metadata: serde_json::json!({
                    "width": 1,
                    "height": 1,
                    "preview": {"mime_type": "image/png"},
                }),
                bytes: b"export bytes".to_vec(),
            },
            &blob_store,
        )
        .expect("payload should insert");

    let mut daemon = blip_daemon_command(&db_path, &socket_path)
        .spawn()
        .expect("daemon should start");
    wait_for_socket(&socket_path);

    blip_command_with_socket(&db_path, &socket_path)
        .args([
            "policy",
            "payload-lab",
            "--agent-raw-payload-access",
            "true",
        ])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("agent raw payload access: true"));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["payload", "inspect", &payload.id])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains(format!("payload: {}", payload.id)))
        .stdout(predicate::str::contains("kind: image"))
        .stdout(predicate::str::contains("has blob: true"));

    let preview_path = temp.path().join("payload-preview.bin");
    blip_command_with_socket(&db_path, &socket_path)
        .args([
            "payload",
            "preview",
            &payload.id,
            preview_path.to_str().expect("path should be utf8"),
        ])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("exported payload"));
    assert_eq!(
        std::fs::read(&preview_path).expect("preview file should read"),
        b"preview bytes"
    );

    let export_path = temp.path().join("payload.bin");
    blip_command_with_socket(&db_path, &socket_path)
        .args([
            "payload",
            "export",
            &payload.id,
            export_path.to_str().expect("path should be utf8"),
        ])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(predicate::str::contains("exported payload"));
    assert_eq!(
        std::fs::read(&export_path).expect("exported file should read"),
        b"export bytes"
    );

    std::fs::write(&export_path, b"existing").expect("existing file should write");
    blip_command_with_socket(&db_path, &socket_path)
        .args([
            "payload",
            "export",
            &payload.id,
            export_path.to_str().expect("path should be utf8"),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "refusing to overwrite existing file",
        ));
    assert_eq!(
        std::fs::read(&export_path).expect("existing file should read"),
        b"existing"
    );

    let json_path = temp.path().join("payload-json.bin");
    let exported = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &[
            "payload",
            "export",
            &payload.id,
            json_path.to_str().expect("path should be utf8"),
            "--output",
            "json",
        ],
    );
    assert_eq!(exported["payload_id"], payload.id);
    assert_eq!(exported["byte_size"], 12);
    assert_eq!(
        std::fs::read(&json_path).expect("json export file should read"),
        b"export bytes"
    );

    blip_command_with_socket(&db_path, &socket_path)
        .args([
            "payload",
            "export",
            &payload.id,
            export_path.to_str().expect("path should be utf8"),
            "--force",
        ])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    assert_eq!(
        std::fs::read(&export_path).expect("forced file should read"),
        b"export bytes"
    );

    stop_daemon(&mut daemon);
}

#[test]
fn health_reports_when_daemon_socket_is_unavailable() {
    let temp = tempdir().expect("tempdir should exist");
    let db_path = temp.path().join("blipcoard-test.db");
    let socket_path = temp.path().join("missing-daemon.sock");

    blip_command_with_socket(&db_path, &socket_path)
        .args(["health", "--output", "json"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "daemon is not running at the configured socket",
        ));
}

#[test]
fn service_status_reports_missing_daemon_without_failing() {
    let temp = tempdir().expect("tempdir should exist");
    let db_path = temp.path().join("blipcoard-test.db");
    let socket_path = temp.path().join("missing-daemon.sock");

    blip_command_with_socket(&db_path, &socket_path)
        .args(["service", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("service: blipd"))
        .stdout(predicate::str::contains("daemon: not running"))
        .stdout(predicate::str::contains(
            "start command: blip service start",
        ))
        .stderr(predicate::str::is_empty());

    let status = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["service", "status", "--output", "json"],
    );
    assert_eq!(status["service"], "blipd");
    assert_eq!(status["daemon_running"], false);
    assert!(
        status["daemon_error"]
            .as_str()
            .expect("daemon error should be a string")
            .contains("daemon is not running")
    );
    assert_eq!(status["socket_path"], socket_path.display().to_string());
}

#[test]
fn service_status_reports_running_daemon_health() {
    let temp = tempdir().expect("tempdir should exist");
    let db_path = temp.path().join("blipcoard-test.db");
    let socket_path = temp.path().join("blipcoard.sock");
    let mut daemon = blip_daemon_command(&db_path, &socket_path)
        .spawn()
        .expect("daemon should start");
    wait_for_socket(&socket_path);

    let status = assert_json_success(
        blip_command_with_socket(&db_path, &socket_path),
        &["service", "status", "--output", "json"],
    );
    assert_eq!(status["service"], "blipd");
    assert_eq!(status["daemon_running"], true);
    assert_eq!(status["daemon_error"], Value::Null);
    assert_eq!(status["database_path"], db_path.display().to_string());

    stop_daemon(&mut daemon);
}

#[test]
fn service_logs_and_plan_explain_platform_paths() {
    let temp = tempdir().expect("tempdir should exist");
    let db_path = temp.path().join("blipcoard-test.db");
    let socket_path = temp.path().join("missing-daemon.sock");

    blip_command_with_socket(&db_path, &socket_path)
        .args(["service", "logs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("blipd"))
        .stderr(predicate::str::is_empty());

    blip_command_with_socket(&db_path, &socket_path)
        .args(["service", "plan"])
        .assert()
        .success()
        .stdout(predicate::str::contains("service: blipd"))
        .stdout(predicate::str::contains("manager:"))
        .stdout(predicate::str::contains("socket:"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn agent_reads_require_the_daemon_socket() {
    let temp = tempdir().expect("tempdir should exist");
    let db_path = temp.path().join("blipcoard-test.db");
    let socket_path = temp.path().join("missing-daemon.sock");

    blip_command(&db_path)
        .args(["create", "agent-feed", "--agent-access"])
        .assert()
        .success();

    blip_command(&db_path)
        .args(["add-demo", "agent-feed", "Agent-visible deployment note"])
        .assert()
        .success();

    blip_command_with_socket(&db_path, &socket_path)
        .args(["agent", "recent", "agent-feed", "--output", "json"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "daemon is not running at the configured socket",
        ));
}

fn assert_json_success(mut cmd: Command, args: &[&str]) -> Value {
    let assert = cmd
        .args(args)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    serde_json::from_slice(&assert.get_output().stdout).expect("stdout should contain JSON")
}

fn wait_for_socket(socket_path: &Path) {
    for _ in 0..200 {
        if socket_path.exists() {
            return;
        }

        thread::sleep(Duration::from_millis(50));
    }

    panic!("daemon socket was not created at {}", socket_path.display());
}

fn stop_daemon(daemon: &mut Child) {
    daemon.kill().ok();
    daemon.wait().ok();
}
