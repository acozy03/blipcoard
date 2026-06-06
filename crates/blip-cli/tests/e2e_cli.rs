use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::{Connection, OptionalExtension};
use tempfile::tempdir;

fn blip_command(db_path: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("blip-cli").expect("binary should build");
    cmd.env("BLIPCOARD_DB_PATH", db_path);
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

    blip_command(&db_path)
        .args(["current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth-bug"));

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
