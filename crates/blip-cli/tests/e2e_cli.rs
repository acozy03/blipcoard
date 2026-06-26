use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use std::path::Path;
use std::process::{Child, Command as ProcessCommand};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

fn blip_command(db_path: &Path) -> Command {
    let mut cmd = Command::cargo_bin("blip-cli").expect("binary should build");
    cmd.env("BLIPCOARD_DB_PATH", db_path);
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
        .stdout(predicate::str::contains("Copied inbox note"));

    blip_command_with_socket(&db_path, &socket_path)
        .args(["list", "auth-bug"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TypeError: broken login flow"));

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

    blip_command_with_socket(&db_path, &socket_path)
        .args(["agent", "recent", "auth-bug"])
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
