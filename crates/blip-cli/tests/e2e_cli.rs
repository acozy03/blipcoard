use assert_cmd::Command;
use predicates::prelude::*;
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
}
