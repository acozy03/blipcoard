use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::{Connection, OptionalExtension};
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
        .args(["use", "auth-bug"])
        .assert()
        .success()
        .stdout(predicate::str::contains("active workspace set to auth-bug"));

    let socket_path = temp.path().join("blipcoard.sock");
    let mut daemon = blip_daemon_command(&db_path, &socket_path)
        .spawn()
        .expect("daemon should start");
    wait_for_socket(&socket_path);

    blip_command_with_socket(&db_path, &socket_path)
        .args(["current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth-bug"));
    stop_daemon(&mut daemon);

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
        .args(["list", "auth-bug"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TypeError: broken login flow"));

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
        .arg("health")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "daemon is not running at the configured socket",
        ));
}

fn wait_for_socket(socket_path: &Path) {
    for _ in 0..100 {
        if socket_path.exists() {
            return;
        }

        thread::sleep(Duration::from_millis(5));
    }

    panic!("daemon socket was not created at {}", socket_path.display());
}

fn stop_daemon(daemon: &mut Child) {
    daemon.kill().ok();
    daemon.wait().ok();
}
