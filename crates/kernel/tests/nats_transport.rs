use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use hydra_kernel::nats::NatsTransportConfig;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn clear_nats_env() {
    for name in [
        "NATS_CREDS_FILE",
        "NATS_REQUIRE_AUTH",
        "NATS_TLS_REQUIRED",
        "NATS_TLS_CA_FILE",
        "NATS_TLS_CLIENT_CERT_FILE",
        "NATS_TLS_CLIENT_KEY_FILE",
    ] {
        env::remove_var(name);
    }
}

fn temp_file(name: &str) -> PathBuf {
    let path = env::temp_dir().join(format!("hydra-nats-{name}-{}", std::process::id()));
    fs::write(&path, b"fixture").expect("write test file");
    path
}

#[test]
fn development_keeps_plain_loopback_compatibility() {
    let _guard = ENV_LOCK.lock().expect("environment lock");
    clear_nats_env();

    let config = NatsTransportConfig::from_environment("nats://127.0.0.1:4222".to_owned(), false)
        .expect("plain dev NATS");
    assert!(!config.require_auth);
    assert!(!config.require_tls);
}

#[test]
fn secure_environment_requires_authentication_and_tls() {
    let _guard = ENV_LOCK.lock().expect("environment lock");
    clear_nats_env();

    let error = NatsTransportConfig::from_environment("nats://nats:4222".to_owned(), true)
        .expect_err("secure defaults must require mounted credentials");
    assert!(error.contains("NATS_CREDS_FILE"));
}

#[test]
fn secure_environment_accepts_pinned_files_and_mtls_pair() {
    let _guard = ENV_LOCK.lock().expect("environment lock");
    clear_nats_env();
    let creds = temp_file("creds");
    let ca = temp_file("ca");
    let cert = temp_file("cert");
    let key = temp_file("key");
    env::set_var("NATS_CREDS_FILE", &creds);
    env::set_var("NATS_TLS_CA_FILE", &ca);
    env::set_var("NATS_TLS_CLIENT_CERT_FILE", &cert);
    env::set_var("NATS_TLS_CLIENT_KEY_FILE", &key);

    let config = NatsTransportConfig::from_environment("tls://nats:4222".to_owned(), true)
        .expect("secure NATS files");
    assert!(config.require_auth);
    assert!(config.require_tls);
    assert_eq!(config.credentials_file.as_deref(), Some(creds.as_path()));

    clear_nats_env();
    for path in [creds, ca, cert, key] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn embedded_url_credentials_are_rejected_without_secret_echo() {
    let _guard = ENV_LOCK.lock().expect("environment lock");
    clear_nats_env();

    let error = NatsTransportConfig::from_environment(
        "nats://hydra:super-secret@nats:4222".to_owned(),
        false,
    )
    .expect_err("URL credentials must be rejected");
    assert!(error.contains("embedded credentials"));
    assert!(!error.contains("super-secret"));
}

#[test]
fn client_certificate_requires_a_key() {
    let _guard = ENV_LOCK.lock().expect("environment lock");
    clear_nats_env();
    let cert = temp_file("cert-only");
    env::set_var("NATS_TLS_CLIENT_CERT_FILE", &cert);

    let error = NatsTransportConfig::from_environment("nats://nats:4222".to_owned(), false)
        .expect_err("certificate without key");
    assert!(error.contains("NATS_TLS_CLIENT_KEY_FILE"));

    clear_nats_env();
    let _ = fs::remove_file(cert);
}
