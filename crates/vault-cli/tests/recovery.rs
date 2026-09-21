use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use bridge_host::EncryptedVault;

const TEST_KEY: &str = "test-vault-passphrase-1234";

fn temp_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("hydra-vault-cli-{stamp}"));
    fs::create_dir_all(&path).expect("create temporary directory");
    path
}

fn run_cli(args: &[&str], path: &PathBuf, confirm: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hydra-vault"));
    command
        .args(args)
        .env("HYDRA_VAULT_PATH", path)
        .env("HYDRA_VAULT_KEY", TEST_KEY)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(value) = confirm {
        command.env("HYDRA_VAULT_RESTORE_CONFIRM", value);
    } else {
        command.env_remove("HYDRA_VAULT_RESTORE_CONFIRM");
    }
    command.output().expect("run hydra-vault")
}

#[test]
fn backup_and_confirmed_restore_use_the_real_cli_without_secret_output() {
    let directory = temp_dir();
    let active = directory.join("active.age");
    let backup = directory.join("backup.age");
    let mut vault = EncryptedVault::new();
    vault
        .set("synthetic_token", "synthetic-secret")
        .expect("valid synthetic secret name");
    vault.save(&active, TEST_KEY).expect("save active vault");

    let backup_output = run_cli(
        &["backup", backup.to_str().expect("backup path")],
        &active,
        None,
    );
    assert!(backup_output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&backup_output.stdout),
        "vault backup: ok\n"
    );
    assert!(!String::from_utf8_lossy(&backup_output.stdout).contains("synthetic-secret"));
    assert!(
        !String::from_utf8_lossy(&fs::read(&backup).expect("read backup"))
            .contains("synthetic-secret")
    );

    let denied = run_cli(
        &["restore", backup.to_str().expect("backup path")],
        &active,
        None,
    );
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("HYDRA_VAULT_RESTORE_CONFIRM"));
    assert!(!String::from_utf8_lossy(&denied.stderr).contains("synthetic-secret"));

    let restored = run_cli(
        &["restore", backup.to_str().expect("backup path")],
        &active,
        Some("restore"),
    );
    assert!(restored.status.success());
    assert_eq!(
        String::from_utf8_lossy(&restored.stdout),
        "vault restore: ok\n"
    );
    assert!(!String::from_utf8_lossy(&restored.stdout).contains("synthetic-secret"));

    fs::remove_dir_all(directory).expect("remove temporary directory");
}
