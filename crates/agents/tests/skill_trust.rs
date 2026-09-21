use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use agents::skills::{
    SkillManifest, SkillRegistry, SkillTrustAnchor, SkillTrustFile, DECLARATIVE_SANDBOX,
    SKILL_MANIFEST_SCHEMA, TRUST_FILE_SCHEMA,
};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAA6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=\n-----END PUBLIC KEY-----\n";
const PRIVATE_KEY_DER: &[u8] = &[
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];

#[test]
fn skill_trust_accepts_upstream_metadata_and_signed_hash() -> Result<(), Box<dyn std::error::Error>>
{
    let root = temp_dir("trusted");
    let trust_path = root.join("trust.json");
    fs::create_dir_all(&root)?;
    write_package(
        &root,
        "migration-helper",
        "1.2.3",
        "hydra-test",
        "key-1",
        vec!["hydra.crm.read"],
        vec!["hydra.crm.context"],
        DECLARATIVE_SANDBOX,
        None,
    )?;
    write_trust_file(&trust_path, &[anchor("hydra-test", "key-1", false)])?;

    let registry = SkillRegistry::load(&root, &trust_path, 1_800_000_000)?;
    let skills = registry.discover();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "migration-helper");
    assert_eq!(skills[0].version, "1.2.3");
    assert_eq!(skills[0].sandbox, DECLARATIVE_SANDBOX);
    assert!(registry.diagnostics().is_empty());
    cleanup(root);
    Ok(())
}

#[test]
fn skill_trust_fails_closed_for_rotation_hash_and_least_authority(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_dir("fail-closed");
    let trust_path = root.join("trust.json");
    fs::create_dir_all(&root)?;
    write_package(
        &root,
        "rotated-skill",
        "1.0.0",
        "hydra-test",
        "key-2",
        vec!["hydra.crm.read"],
        vec!["hydra.crm.context"],
        DECLARATIVE_SANDBOX,
        None,
    )?;
    write_package(
        &root,
        "unknown-signer",
        "1.0.0",
        "unknown",
        "key-1",
        vec!["hydra.crm.read"],
        vec!["hydra.crm.context"],
        DECLARATIVE_SANDBOX,
        None,
    )?;
    write_package(
        &root,
        "revoked-skill",
        "1.0.0",
        "hydra-test",
        "key-1",
        vec!["hydra.crm.read"],
        vec!["hydra.crm.context"],
        DECLARATIVE_SANDBOX,
        None,
    )?;
    write_package(
        &root,
        "authority-skill",
        "1.0.0",
        "hydra-test",
        "key-2",
        vec!["hydra.envelopes.approve"],
        vec!["hydra.crm.context"],
        "shell",
        Some("shell:run"),
    )?;
    write_package(
        &root,
        "tampered-skill",
        "1.0.0",
        "hydra-test",
        "key-2",
        vec!["hydra.crm.read"],
        vec!["hydra.crm.context"],
        DECLARATIVE_SANDBOX,
        None,
    )?;
    fs::write(
        root.join("tampered-skill").join("SKILL.md"),
        "---\nname: tampered-skill\ndescription: Changed after signing.\n---\n# Tampered\n",
    )?;
    write_trust_file(
        &trust_path,
        &[
            anchor("hydra-test", "key-1", true),
            anchor("hydra-test", "key-2", false),
        ],
    )?;

    let registry = SkillRegistry::load(&root, &trust_path, 1_800_000_000)?;
    assert_eq!(
        registry
            .discover()
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>(),
        ["rotated-skill"]
    );
    assert_eq!(registry.diagnostics().len(), 4);
    assert!(registry
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.reason.contains("not trusted")));
    assert!(registry
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.reason.contains("revoked")));
    assert!(registry
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.reason.contains("SHA-256")));
    assert!(registry.diagnostics().iter().any(|diagnostic| {
        diagnostic.reason.contains("scope")
            || diagnostic.reason.contains("sandbox")
            || diagnostic.reason.contains("tools")
    }));
    cleanup(root);
    Ok(())
}

#[test]
fn skill_trust_rejects_duplicate_name_and_version_across_roots(
) -> Result<(), Box<dyn std::error::Error>> {
    let root_a = temp_dir("duplicate-a");
    let root_b = temp_dir("duplicate-b");
    let trust_path = root_a.join("trust.json");
    fs::create_dir_all(&root_a)?;
    fs::create_dir_all(&root_b)?;
    write_package(
        &root_a,
        "duplicate-skill",
        "1.0.0",
        "hydra-test",
        "key-1",
        vec!["hydra.crm.read"],
        vec!["hydra.crm.context"],
        DECLARATIVE_SANDBOX,
        None,
    )?;
    write_package(
        &root_b,
        "duplicate-skill",
        "1.0.0",
        "hydra-test",
        "key-1",
        vec!["hydra.crm.read"],
        vec!["hydra.crm.context"],
        DECLARATIVE_SANDBOX,
        None,
    )?;
    write_trust_file(&trust_path, &[anchor("hydra-test", "key-1", false)])?;

    let result = SkillRegistry::load_roots(
        &[root_a.as_path(), root_b.as_path()],
        &trust_path,
        1_800_000_000,
    );
    assert!(result
        .err()
        .is_some_and(|error| error.to_string().contains("duplicate-skill@1.0.0")));
    cleanup(root_a);
    cleanup(root_b);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_package(
    root: &Path,
    name: &str,
    version: &str,
    signer: &str,
    key_id: &str,
    scopes: Vec<&str>,
    capabilities: Vec<&str>,
    sandbox: &str,
    allowed_tools: Option<&str>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let package = root.join(name);
    fs::create_dir_all(&package)?;
    let allowed_tools = allowed_tools
        .map(|value| format!("allowed-tools: {value}\n"))
        .unwrap_or_default();
    let skill = format!(
        "---\nname: {name}\ndescription: Deterministic {name} fixture.\nlicense: Apache-2.0\ncompatibility: Hydra signed declarative skill fixture.\n{allowed_tools}metadata:\n  owner: hydra\n---\n# {name}\n\nDeclarative skill metadata only.\n"
    );
    fs::write(package.join("SKILL.md"), skill.as_bytes())?;
    let mut manifest = SkillManifest {
        schema_version: SKILL_MANIFEST_SCHEMA.to_owned(),
        name: name.to_owned(),
        version: version.to_owned(),
        signer: signer.to_owned(),
        key_id: key_id.to_owned(),
        skill_sha256: sha256_hex(skill.as_bytes()),
        scopes: scopes.into_iter().map(str::to_owned).collect(),
        capabilities: capabilities.into_iter().map(str::to_owned).collect(),
        sandbox: sandbox.to_owned(),
        issued_at: 1_700_000_000,
        expires_at: 2_000_000_000,
        signature: String::new(),
    };
    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(key_id.to_owned());
    manifest.signature = encode(
        &header,
        &manifest.claims(),
        &EncodingKey::from_ed_der(PRIVATE_KEY_DER),
    )?;
    fs::write(
        package.join("hydra-skill.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(package)
}

fn anchor(signer: &str, key_id: &str, revoked: bool) -> SkillTrustAnchor {
    SkillTrustAnchor {
        signer: signer.to_owned(),
        key_id: key_id.to_owned(),
        algorithm: "EdDSA".to_owned(),
        public_key_pem: PUBLIC_KEY_PEM.to_owned(),
        revoked,
        not_before: None,
        not_after: None,
    }
}

fn write_trust_file(
    path: &Path,
    anchors: &[SkillTrustAnchor],
) -> Result<(), Box<dyn std::error::Error>> {
    let trust = SkillTrustFile {
        schema_version: TRUST_FILE_SCHEMA.to_owned(),
        anchors: anchors.to_vec(),
        allowed_scopes: BTreeSet::from(["hydra.crm.read".to_owned()]),
        allowed_capabilities: BTreeSet::from(["hydra.crm.context".to_owned()]),
        sandbox_profiles: BTreeSet::from([DECLARATIVE_SANDBOX.to_owned()]),
    };
    fs::write(path, serde_json::to_vec_pretty(&trust)?)?;
    Ok(())
}

fn temp_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hydra-skill-{label}-{}", Uuid::new_v4()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn cleanup(path: PathBuf) {
    let _ = fs::remove_dir_all(path);
}
