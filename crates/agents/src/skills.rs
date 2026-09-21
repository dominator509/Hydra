use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::str;

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SKILL_MANIFEST_SCHEMA: &str = "hydra.skill.v1";
pub const TRUST_FILE_SCHEMA: &str = "hydra.skill-trust.v1";
pub const DECLARATIVE_SANDBOX: &str = "declarative-only";
const MAX_SIGNATURE_BYTES: usize = 8 * 1024;
const MAX_FUTURE_SKEW_SECONDS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub name: String,
    pub version: String,
    pub signer: String,
    #[serde(rename = "keyId")]
    pub key_id: String,
    #[serde(rename = "skillSha256")]
    pub skill_sha256: String,
    pub scopes: Vec<String>,
    pub capabilities: Vec<String>,
    pub sandbox: String,
    #[serde(rename = "issuedAt")]
    pub issued_at: i64,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillManifestClaims {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub name: String,
    pub version: String,
    pub signer: String,
    #[serde(rename = "keyId")]
    pub key_id: String,
    #[serde(rename = "skillSha256")]
    pub skill_sha256: String,
    pub scopes: Vec<String>,
    pub capabilities: Vec<String>,
    pub sandbox: String,
    #[serde(rename = "issuedAt")]
    pub issued_at: i64,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
}

impl SkillManifest {
    pub fn claims(&self) -> SkillManifestClaims {
        SkillManifestClaims {
            schema_version: self.schema_version.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            signer: self.signer.clone(),
            key_id: self.key_id.clone(),
            skill_sha256: self.skill_sha256.clone(),
            scopes: self.scopes.clone(),
            capabilities: self.capabilities.clone(),
            sandbox: self.sandbox.clone(),
            issued_at: self.issued_at,
            expires_at: self.expires_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillTrustAnchor {
    pub signer: String,
    #[serde(rename = "keyId")]
    pub key_id: String,
    pub algorithm: String,
    #[serde(rename = "publicKeyPem")]
    pub public_key_pem: String,
    #[serde(default)]
    pub revoked: bool,
    #[serde(rename = "notBefore", default)]
    pub not_before: Option<i64>,
    #[serde(rename = "notAfter", default)]
    pub not_after: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillTrustFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub anchors: Vec<SkillTrustAnchor>,
    #[serde(rename = "allowedScopes")]
    pub allowed_scopes: BTreeSet<String>,
    #[serde(rename = "allowedCapabilities")]
    pub allowed_capabilities: BTreeSet<String>,
    #[serde(rename = "sandboxProfiles")]
    pub sandbox_profiles: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDescriptor {
    pub name: String,
    pub version: String,
    pub description: String,
    pub signer: String,
    #[serde(rename = "keyId")]
    pub key_id: String,
    #[serde(rename = "skillSha256")]
    pub skill_sha256: String,
    pub scopes: Vec<String>,
    pub capabilities: Vec<String>,
    pub sandbox: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDiagnostic {
    pub package: String,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum SkillTrustError {
    #[error("failed to access {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid skill trust file: {0}")]
    InvalidTrustFile(String),
    #[error("duplicate trusted skill {name}@{version}")]
    DuplicateSkill { name: String, version: String },
    #[error("invalid skill package: {0}")]
    InvalidPackage(String),
}

#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: BTreeMap<(String, String), SkillDescriptor>,
    diagnostics: Vec<SkillDiagnostic>,
}

impl SkillRegistry {
    pub fn load(
        root: impl AsRef<Path>,
        trust_file: impl AsRef<Path>,
        now_unix: i64,
    ) -> Result<Self, SkillTrustError> {
        let root = root.as_ref();
        let roots = [root];
        Self::load_roots(&roots, trust_file.as_ref(), now_unix)
    }

    pub fn load_roots(
        roots: &[&Path],
        trust_file: &Path,
        now_unix: i64,
    ) -> Result<Self, SkillTrustError> {
        let trust = read_trust_file(trust_file)?;
        validate_trust_file(&trust)?;
        let mut packages = Vec::new();
        for root in roots {
            ensure_directory(root)?;
            let entries = fs::read_dir(root).map_err(|source| SkillTrustError::Io {
                path: root.display().to_string(),
                source,
            })?;
            for entry in entries {
                let entry = entry.map_err(|source| SkillTrustError::Io {
                    path: root.display().to_string(),
                    source,
                })?;
                let file_type = entry.file_type().map_err(|source| SkillTrustError::Io {
                    path: entry.path().display().to_string(),
                    source,
                })?;
                if file_type.is_dir() && !file_type.is_symlink() {
                    packages.push(entry.path());
                }
            }
        }
        packages.sort();

        let mut registry = Self::default();
        for package in packages {
            match verify_package(&package, &trust, now_unix) {
                Ok(descriptor) => {
                    let key = (descriptor.name.clone(), descriptor.version.clone());
                    if registry.skills.insert(key.clone(), descriptor).is_some() {
                        return Err(SkillTrustError::DuplicateSkill {
                            name: key.0,
                            version: key.1,
                        });
                    }
                }
                Err(error) => registry.diagnostics.push(SkillDiagnostic {
                    package: package.display().to_string(),
                    reason: error.to_string(),
                }),
            }
        }
        Ok(registry)
    }

    pub fn discover(&self) -> Vec<SkillDescriptor> {
        self.skills.values().cloned().collect()
    }

    pub fn diagnostics(&self) -> &[SkillDiagnostic] {
        &self.diagnostics
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    compatibility: Option<String>,
    #[serde(rename = "allowed-tools", default)]
    allowed_tools: Option<String>,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
}

fn read_trust_file(path: &Path) -> Result<SkillTrustFile, SkillTrustError> {
    let bytes = read_regular_file(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| SkillTrustError::InvalidTrustFile(error.to_string()))
}

fn validate_trust_file(trust: &SkillTrustFile) -> Result<(), SkillTrustError> {
    if trust.schema_version != TRUST_FILE_SCHEMA {
        return Err(SkillTrustError::InvalidTrustFile(format!(
            "unsupported schema version '{}'",
            trust.schema_version
        )));
    }
    if trust.anchors.is_empty() {
        return Err(SkillTrustError::InvalidTrustFile(
            "at least one trust anchor is required".to_owned(),
        ));
    }
    if !trust.sandbox_profiles.contains(DECLARATIVE_SANDBOX) {
        return Err(SkillTrustError::InvalidTrustFile(
            "declarative-only sandbox policy is required".to_owned(),
        ));
    }
    let mut anchor_ids = BTreeSet::new();
    for anchor in &trust.anchors {
        if anchor.signer.trim().is_empty()
            || anchor.key_id.trim().is_empty()
            || anchor.algorithm != "EdDSA"
            || anchor.public_key_pem.trim().is_empty()
        {
            return Err(SkillTrustError::InvalidTrustFile(
                "trust anchors require EdDSA, signer, keyId, and publicKeyPem".to_owned(),
            ));
        }
        if !anchor_ids.insert((anchor.signer.clone(), anchor.key_id.clone())) {
            return Err(SkillTrustError::InvalidTrustFile(
                "duplicate signer/keyId trust anchor".to_owned(),
            ));
        }
    }
    Ok(())
}

fn verify_package(
    package: &Path,
    trust: &SkillTrustFile,
    now_unix: i64,
) -> Result<SkillDescriptor, SkillTrustError> {
    let package_name = package
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| SkillTrustError::InvalidPackage("package name is not UTF-8".to_owned()))?;
    let skill_path = package.join("SKILL.md");
    let manifest_path = package.join("hydra-skill.json");
    let skill_bytes = read_regular_file(&skill_path)?;
    let manifest_bytes = read_regular_file(&manifest_path)?;
    let manifest: SkillManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| SkillTrustError::InvalidPackage(format!("manifest JSON: {error}")))?;
    let frontmatter = parse_frontmatter(&skill_bytes)?;

    if package_name != manifest.name {
        return Err(SkillTrustError::InvalidPackage(
            "manifest name must match the package directory".to_owned(),
        ));
    }
    validate_manifest(&manifest, &frontmatter, now_unix)?;
    verify_signature(&manifest, trust, now_unix)?;
    if manifest.skill_sha256 != sha256_hex(&skill_bytes) {
        return Err(SkillTrustError::InvalidPackage(
            "SKILL.md SHA-256 does not match the signed manifest".to_owned(),
        ));
    }
    let requested_scopes = manifest.scopes.iter().cloned().collect::<BTreeSet<_>>();
    if !trust.allowed_scopes.is_superset(&requested_scopes) {
        return Err(SkillTrustError::InvalidPackage(
            "manifest requests a scope outside the explicit trust policy".to_owned(),
        ));
    }
    let requested_capabilities = manifest
        .capabilities
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if !trust
        .allowed_capabilities
        .is_superset(&requested_capabilities)
    {
        return Err(SkillTrustError::InvalidPackage(
            "manifest requests a capability outside the explicit trust policy".to_owned(),
        ));
    }
    if !trust.sandbox_profiles.contains(&manifest.sandbox) {
        return Err(SkillTrustError::InvalidPackage(
            "manifest sandbox policy is not trusted".to_owned(),
        ));
    }

    Ok(SkillDescriptor {
        name: manifest.name,
        version: manifest.version,
        description: frontmatter.description,
        signer: manifest.signer,
        key_id: manifest.key_id,
        skill_sha256: manifest.skill_sha256,
        scopes: manifest.scopes,
        capabilities: manifest.capabilities,
        sandbox: manifest.sandbox,
    })
}

fn validate_manifest(
    manifest: &SkillManifest,
    frontmatter: &SkillFrontmatter,
    now_unix: i64,
) -> Result<(), SkillTrustError> {
    if manifest.schema_version != SKILL_MANIFEST_SCHEMA {
        return Err(SkillTrustError::InvalidPackage(
            "unsupported manifest schema version".to_owned(),
        ));
    }
    validate_skill_name(&manifest.name)?;
    if frontmatter.name != manifest.name {
        return Err(SkillTrustError::InvalidPackage(
            "SKILL.md frontmatter name does not match the signed manifest".to_owned(),
        ));
    }
    if !valid_semver(&manifest.version) {
        return Err(SkillTrustError::InvalidPackage(
            "skill version must be semantic version 2.0.0 compatible".to_owned(),
        ));
    }
    if frontmatter.description.trim().is_empty() || frontmatter.description.len() > 1024 {
        return Err(SkillTrustError::InvalidPackage(
            "SKILL.md description is empty or too long".to_owned(),
        ));
    }
    if frontmatter
        .license
        .as_deref()
        .is_some_and(|value| value.len() > 128)
        || frontmatter
            .compatibility
            .as_deref()
            .is_some_and(|value| value.len() > 500)
        || frontmatter
            .metadata
            .iter()
            .any(|(key, value)| key.is_empty() || key.len() > 128 || value.len() > 512)
    {
        return Err(SkillTrustError::InvalidPackage(
            "SKILL.md metadata exceeds the supported bound".to_owned(),
        ));
    }
    if frontmatter
        .allowed_tools
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(SkillTrustError::InvalidPackage(
            "skills cannot grant tools through allowed-tools".to_owned(),
        ));
    }
    if manifest.signer.trim().is_empty()
        || manifest.signer.len() > 128
        || manifest.key_id.trim().is_empty()
        || manifest.key_id.len() > 128
        || manifest.signature.len() > MAX_SIGNATURE_BYTES
        || manifest.signature.trim().is_empty()
        || manifest.skill_sha256.len() != 64
        || !manifest
            .skill_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(SkillTrustError::InvalidPackage(
            "manifest identity, signature, or hash is invalid".to_owned(),
        ));
    }
    if manifest.issued_at > now_unix.saturating_add(MAX_FUTURE_SKEW_SECONDS)
        || manifest.expires_at <= now_unix
        || manifest.expires_at <= manifest.issued_at
    {
        return Err(SkillTrustError::InvalidPackage(
            "manifest validity window is not active".to_owned(),
        ));
    }
    if has_duplicates(&manifest.scopes) || has_duplicates(&manifest.capabilities) {
        return Err(SkillTrustError::InvalidPackage(
            "manifest scope and capability lists must not contain duplicates".to_owned(),
        ));
    }
    Ok(())
}

fn verify_signature(
    manifest: &SkillManifest,
    trust: &SkillTrustFile,
    now_unix: i64,
) -> Result<(), SkillTrustError> {
    let header = decode_header(&manifest.signature).map_err(|_| {
        SkillTrustError::InvalidPackage("manifest signature is not a valid JWS".to_owned())
    })?;
    if header.alg != Algorithm::EdDSA || header.kid.as_deref() != Some(manifest.key_id.as_str()) {
        return Err(SkillTrustError::InvalidPackage(
            "manifest JWS algorithm or keyId is not allowed".to_owned(),
        ));
    }
    let anchor = trust
        .anchors
        .iter()
        .find(|anchor| anchor.signer == manifest.signer && anchor.key_id == manifest.key_id)
        .ok_or_else(|| SkillTrustError::InvalidPackage("signer/keyId is not trusted".to_owned()))?;
    if anchor.revoked
        || anchor
            .not_before
            .is_some_and(|not_before| now_unix < not_before)
        || anchor
            .not_after
            .is_some_and(|not_after| now_unix >= not_after)
    {
        return Err(SkillTrustError::InvalidPackage(
            "signer trust anchor is revoked or outside its validity window".to_owned(),
        ));
    }
    let key = DecodingKey::from_ed_pem(anchor.public_key_pem.as_bytes()).map_err(|_| {
        SkillTrustError::InvalidPackage("trusted Ed25519 public key is invalid".to_owned())
    })?;
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = false;
    validation.required_spec_claims.clear();
    let decoded =
        decode::<SkillManifestClaims>(&manifest.signature, &key, &validation).map_err(|_| {
            SkillTrustError::InvalidPackage("manifest signature verification failed".to_owned())
        })?;
    if decoded.claims != manifest.claims() {
        return Err(SkillTrustError::InvalidPackage(
            "signed manifest claims do not match the package manifest".to_owned(),
        ));
    }
    Ok(())
}

fn parse_frontmatter(bytes: &[u8]) -> Result<SkillFrontmatter, SkillTrustError> {
    let text = str::from_utf8(bytes)
        .map_err(|_| SkillTrustError::InvalidPackage("SKILL.md must be UTF-8".to_owned()))?;
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err(SkillTrustError::InvalidPackage(
            "SKILL.md must start with YAML frontmatter".to_owned(),
        ));
    }
    let mut yaml = String::new();
    let mut closed = false;
    for line in &mut lines {
        if line == "---" {
            closed = true;
            break;
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    let body_has_content = lines.any(|line| !line.trim().is_empty());
    if !closed || !body_has_content {
        return Err(SkillTrustError::InvalidPackage(
            "SKILL.md frontmatter or body is missing".to_owned(),
        ));
    }
    serde_yaml::from_str(&yaml).map_err(|error| {
        SkillTrustError::InvalidPackage(format!("invalid SKILL.md frontmatter: {error}"))
    })
}

fn ensure_directory(path: &Path) -> Result<(), SkillTrustError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| SkillTrustError::Io {
        path: path.display().to_string(),
        source,
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(SkillTrustError::InvalidTrustFile(format!(
            "skill root is not a non-symlink directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>, SkillTrustError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| SkillTrustError::Io {
        path: path.display().to_string(),
        source,
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(SkillTrustError::InvalidPackage(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    fs::read(path).map_err(|source| SkillTrustError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn validate_skill_name(name: &str) -> Result<(), SkillTrustError> {
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(SkillTrustError::InvalidPackage(
            "skill name is not valid lowercase hyphenated metadata".to_owned(),
        ));
    }
    Ok(())
}

fn valid_semver(value: &str) -> bool {
    let core = value.split(['-', '+']).next().unwrap_or_default();
    let parts = core.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && (part == &"0" || !part.starts_with('0'))
                && part.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn has_duplicates(values: &[String]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() != values.len()
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
