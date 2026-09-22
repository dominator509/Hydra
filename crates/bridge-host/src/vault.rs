use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use age::secrecy::SecretString;
use age::{Decryptor, Encryptor, Identity};
use anyhow::Result;
use async_trait::async_trait;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::host::SecretSource;

pub const DEFAULT_VAULT_PATH: &str = "data/vault.age";

const FORMAT_VERSION: u8 = 1;
const MAX_ENCRYPTED_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PLAINTEXT_BYTES: usize = 1024 * 1024;
const MAX_SECRET_COUNT: usize = 256;
const MAX_SECRET_NAME_BYTES: usize = 128;
const MAX_SECRET_VALUE_BYTES: usize = 256 * 1024;
const VALID_NAME_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-";

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("vault I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("vault decryption failed: {0}")]
    Decryption(#[source] age::DecryptError),
    #[error("vault encryption stream failed: {0}")]
    EncryptionIo(#[source] io::Error),
    #[error("vault decryption stream failed: {0}")]
    DecryptionIo(#[source] io::Error),
    #[error("vault document is invalid: {0}")]
    InvalidDocument(String),
    #[error("vault secret name is invalid: {0}")]
    InvalidName(String),
    #[error("vault passphrase must be at least 16 bytes")]
    WeakPassphrase,
    #[error("vault document exceeds the supported size")]
    TooLarge,
    #[error("vault destination already exists: {path}")]
    DestinationExists { path: String },
}

impl VaultError {
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::Io { source, .. } if source.kind() == io::ErrorKind::NotFound
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct VaultDocument {
    version: u8,
    #[serde(deserialize_with = "deserialize_unique_secrets")]
    secrets: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct EncryptedVault {
    secrets: BTreeMap<String, String>,
}

impl Default for EncryptedVault {
    fn default() -> Self {
        Self::new()
    }
}

impl EncryptedVault {
    pub fn new() -> Self {
        Self {
            secrets: BTreeMap::new(),
        }
    }

    pub fn load(path: impl AsRef<Path>, passphrase: &str) -> Result<Self, VaultError> {
        validate_passphrase(passphrase)?;
        let path = path.as_ref();
        let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
        if metadata.len() > MAX_ENCRYPTED_BYTES {
            return Err(VaultError::TooLarge);
        }
        let encrypted = fs::read(path).map_err(|source| io_error(path, source))?;
        Self::from_bytes(&encrypted, passphrase)
    }

    pub fn from_bytes(encrypted: &[u8], passphrase: &str) -> Result<Self, VaultError> {
        validate_passphrase(passphrase)?;
        if encrypted.len() as u64 > MAX_ENCRYPTED_BYTES {
            return Err(VaultError::TooLarge);
        }

        let decryptor = Decryptor::new(encrypted).map_err(VaultError::Decryption)?;
        let identity = age::scrypt::Identity::new(SecretString::from(passphrase.to_owned()));
        let reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn Identity))
            .map_err(VaultError::Decryption)?;
        let mut plaintext = Vec::new();
        reader
            .take((MAX_PLAINTEXT_BYTES + 1) as u64)
            .read_to_end(&mut plaintext)
            .map_err(VaultError::DecryptionIo)?;
        if plaintext.len() > MAX_PLAINTEXT_BYTES {
            return Err(VaultError::TooLarge);
        }

        let document: VaultDocument = serde_json::from_slice(&plaintext)
            .map_err(|error| VaultError::InvalidDocument(error.to_string()))?;
        validate_document(document)
    }

    pub fn save(&self, path: impl AsRef<Path>, passphrase: &str) -> Result<(), VaultError> {
        validate_passphrase(passphrase)?;
        validate_document(VaultDocument {
            version: FORMAT_VERSION,
            secrets: self.secrets.clone(),
        })?;
        let encrypted = self.to_bytes(passphrase)?;
        atomic_write(path.as_ref(), &encrypted, true)
    }

    pub fn backup_to(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
        passphrase: &str,
    ) -> Result<(), VaultError> {
        let source = source.as_ref();
        let destination = destination.as_ref();
        Self::validate_copy_source(source, destination, passphrase)?;
        if destination.exists() {
            return Err(VaultError::DestinationExists {
                path: destination.display().to_string(),
            });
        }
        let encrypted = fs::read(source).map_err(|error| io_error(source, error))?;
        atomic_write(destination, &encrypted, false)
    }

    pub fn restore_to(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
        passphrase: &str,
    ) -> Result<(), VaultError> {
        let source = source.as_ref();
        let destination = destination.as_ref();
        Self::validate_copy_source(source, destination, passphrase)?;
        let encrypted = fs::read(source).map_err(|error| io_error(source, error))?;
        atomic_write(destination, &encrypted, true)
    }

    fn validate_copy_source(
        source: &Path,
        destination: &Path,
        passphrase: &str,
    ) -> Result<(), VaultError> {
        Self::load(source, passphrase)?;
        if source == destination {
            return Err(VaultError::InvalidDocument(
                "vault source and destination must differ".to_owned(),
            ));
        }
        let source_canonical = fs::canonicalize(source).map_err(|error| io_error(source, error))?;
        if let Ok(destination_canonical) = fs::canonicalize(destination) {
            if source_canonical == destination_canonical {
                return Err(VaultError::InvalidDocument(
                    "vault source and destination must differ".to_owned(),
                ));
            }
        }
        Ok(())
    }

    pub fn set(&mut self, name: &str, value: &str) -> Result<(), VaultError> {
        validate_name(name)?;
        if value.is_empty() {
            return Err(VaultError::InvalidDocument(
                "secret values must not be empty".to_owned(),
            ));
        }
        if value.len() > MAX_SECRET_VALUE_BYTES {
            return Err(VaultError::TooLarge);
        }
        if !self.secrets.contains_key(name) && self.secrets.len() >= MAX_SECRET_COUNT {
            return Err(VaultError::TooLarge);
        }
        self.secrets.insert(name.to_owned(), value.to_owned());
        Ok(())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.secrets.keys().map(String::as_str)
    }

    pub fn secret_source(&self) -> VaultSecretSource {
        VaultSecretSource {
            values: Arc::new(self.secrets.clone()),
        }
    }

    fn to_bytes(&self, passphrase: &str) -> Result<Vec<u8>, VaultError> {
        let document = VaultDocument {
            version: FORMAT_VERSION,
            secrets: self.secrets.clone(),
        };
        let plaintext = serde_json::to_vec(&document)
            .map_err(|error| VaultError::InvalidDocument(error.to_string()))?;
        if plaintext.len() > MAX_PLAINTEXT_BYTES {
            return Err(VaultError::TooLarge);
        }

        let encryptor = Encryptor::with_user_passphrase(SecretString::from(passphrase.to_owned()));
        let mut encrypted = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut encrypted)
            .map_err(VaultError::EncryptionIo)?;
        writer
            .write_all(&plaintext)
            .map_err(VaultError::EncryptionIo)?;
        writer.finish().map_err(VaultError::EncryptionIo)?;
        Ok(encrypted)
    }
}

#[derive(Clone, Debug)]
pub struct VaultSecretSource {
    values: Arc<BTreeMap<String, String>>,
}

impl VaultSecretSource {
    pub fn load(path: impl AsRef<Path>, passphrase: &str) -> Result<Self, VaultError> {
        Ok(EncryptedVault::load(path, passphrase)?.secret_source())
    }
}

#[async_trait]
impl SecretSource for VaultSecretSource {
    async fn get(&self, name: &str) -> Result<Option<String>> {
        Ok(self.values.get(name).cloned())
    }
}

fn validate_passphrase(passphrase: &str) -> Result<(), VaultError> {
    if passphrase.trim().len() < 16 {
        return Err(VaultError::WeakPassphrase);
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), VaultError> {
    if name.is_empty()
        || name.len() > MAX_SECRET_NAME_BYTES
        || name.trim() != name
        || name
            .chars()
            .any(|character| !VALID_NAME_CHARS.contains(character))
    {
        return Err(VaultError::InvalidName(name.to_owned()));
    }
    Ok(())
}

fn deserialize_unique_secrets<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct UniqueSecretsVisitor;

    impl<'de> Visitor<'de> for UniqueSecretsVisitor {
        type Value = BTreeMap<String, String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a JSON object with unique secret names")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut secrets = BTreeMap::new();
            while let Some(name) = map.next_key::<String>()? {
                let value = map.next_value::<String>()?;
                if secrets.insert(name.clone(), value).is_some() {
                    return Err(de::Error::custom(format!("duplicate secret name '{name}'")));
                }
            }
            Ok(secrets)
        }
    }

    deserializer.deserialize_map(UniqueSecretsVisitor)
}

fn validate_document(document: VaultDocument) -> Result<EncryptedVault, VaultError> {
    if document.version != FORMAT_VERSION {
        return Err(VaultError::InvalidDocument(format!(
            "unsupported version {}",
            document.version
        )));
    }
    if document.secrets.len() > MAX_SECRET_COUNT {
        return Err(VaultError::TooLarge);
    }
    for (name, value) in &document.secrets {
        validate_name(name)?;
        if value.is_empty() || value.len() > MAX_SECRET_VALUE_BYTES {
            return Err(VaultError::InvalidDocument(format!(
                "invalid value size for secret '{name}'"
            )));
        }
    }
    Ok(EncryptedVault {
        secrets: document.secrets,
    })
}

fn io_error(path: &Path, source: io::Error) -> VaultError {
    VaultError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn atomic_write(path: &Path, contents: &[u8], replace_existing: bool) -> Result<(), VaultError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            VaultError::InvalidDocument("vault path has no valid file name".to_owned())
        })?;
    let temp_name = format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let temp_path = parent.join(temp_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|source| io_error(&temp_path, source))?;

    if let Err(error) = write_and_sync(&mut file, contents) {
        let _ = fs::remove_file(&temp_path);
        return Err(io_error(&temp_path, error));
    }
    drop(file);

    if let Err(error) = replace_file(&temp_path, path, replace_existing) {
        let _ = fs::remove_file(&temp_path);
        return Err(io_error(path, error));
    }
    Ok(())
}

fn write_and_sync(file: &mut File, contents: &[u8]) -> io::Result<()> {
    file.write_all(contents)?;
    file.flush()?;
    file.sync_all()
}

fn replace_file(temp_path: &Path, path: &Path, replace_existing: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(temp_path, fs::Permissions::from_mode(0o600))?;
        if replace_existing {
            return fs::rename(temp_path, path);
        }
        fs::hard_link(temp_path, path)?;
        fs::remove_file(temp_path)
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };

        let source: Vec<u16> = temp_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let destination: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let flags = MOVEFILE_WRITE_THROUGH
            | if replace_existing {
                MOVEFILE_REPLACE_EXISTING
            } else {
                0
            };
        let replaced = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) };
        if replaced == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    {
        if !replace_existing && path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "vault destination already exists",
            ));
        }
        fs::rename(temp_path, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_KEY: &str = "test-vault-passphrase-1234";

    fn temp_path(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("hydra-vault-{label}-{stamp}.age"))
    }

    #[test]
    fn encrypted_round_trip_and_names_are_deterministic() {
        let path = temp_path("round-trip");
        let mut vault = EncryptedVault::new();
        vault
            .set("suitecrm_client_secret", "synthetic-secret")
            .expect("valid synthetic secret name");
        vault
            .set("smtp_password", "synthetic-password")
            .expect("valid synthetic secret name");
        vault.save(&path, TEST_KEY).expect("save test vault");

        let raw = fs::read(&path).expect("read test vault");
        assert!(!String::from_utf8_lossy(&raw).contains("synthetic-secret"));
        let loaded = EncryptedVault::load(&path, TEST_KEY).expect("load test vault");
        assert_eq!(
            loaded.names().collect::<Vec<_>>(),
            vec!["smtp_password", "suitecrm_client_secret"]
        );
        fs::remove_file(path).expect("remove test vault");
    }

    #[test]
    fn wrong_key_and_tampering_fail_closed() {
        let path = temp_path("tamper");
        let mut vault = EncryptedVault::new();
        vault
            .set("token", "synthetic-token")
            .expect("valid synthetic secret name");
        vault.save(&path, TEST_KEY).expect("save test vault");
        let mut raw = fs::read(&path).expect("read test vault");
        let last = raw.len() - 1;
        raw[last] ^= 1;
        assert!(EncryptedVault::from_bytes(&raw, TEST_KEY).is_err());
        assert!(EncryptedVault::load(&path, "wrong-vault-passphrase").is_err());
        fs::remove_file(path).expect("remove test vault");
    }

    #[test]
    fn verified_backup_and_restore_preserve_ciphertext() {
        let source = temp_path("recovery-source");
        let backup = temp_path("recovery-backup");
        let active = temp_path("recovery-active");
        let mut vault = EncryptedVault::new();
        vault
            .set("synthetic_token", "synthetic-secret")
            .expect("valid synthetic secret name");
        vault.save(&source, TEST_KEY).expect("save source vault");
        let source_bytes = fs::read(&source).expect("read source vault");

        EncryptedVault::backup_to(&source, &backup, TEST_KEY).expect("backup vault");
        assert_eq!(source_bytes, fs::read(&backup).expect("read backup vault"));

        let mut replacement = EncryptedVault::new();
        replacement
            .set("replacement", "replacement-secret")
            .expect("valid replacement name");
        replacement
            .save(&active, TEST_KEY)
            .expect("save active vault");
        EncryptedVault::restore_to(&backup, &active, TEST_KEY).expect("restore vault");
        assert_eq!(
            source_bytes,
            fs::read(&active).expect("read restored vault")
        );
        assert_eq!(
            EncryptedVault::load(&active, TEST_KEY)
                .expect("load restored vault")
                .names()
                .collect::<Vec<_>>(),
            vec!["synthetic_token"]
        );

        for path in [&source, &backup, &active] {
            fs::remove_file(path).expect("remove recovery fixture");
        }
    }

    #[test]
    fn verified_recovery_rejects_collision_existing_backup_and_wrong_key() {
        let source = temp_path("recovery-errors-source");
        let backup = temp_path("recovery-errors-backup");
        let active = temp_path("recovery-errors-active");
        EncryptedVault::new()
            .save(&source, TEST_KEY)
            .expect("save source vault");
        EncryptedVault::new()
            .save(&active, TEST_KEY)
            .expect("save active vault");
        let active_before = fs::read(&active).expect("read active vault");

        assert!(matches!(
            EncryptedVault::backup_to(&source, &source, TEST_KEY),
            Err(VaultError::InvalidDocument(message)) if message.contains("must differ")
        ));
        EncryptedVault::new()
            .save(&backup, TEST_KEY)
            .expect("save existing backup");
        assert!(matches!(
            EncryptedVault::backup_to(&source, &backup, TEST_KEY),
            Err(VaultError::DestinationExists { .. })
        ));
        assert!(EncryptedVault::restore_to(&source, &active, "wrong-vault-passphrase").is_err());
        assert_eq!(
            active_before,
            fs::read(&active).expect("read unchanged active vault")
        );

        for path in [&source, &backup, &active] {
            fs::remove_file(path).expect("remove recovery error fixture");
        }
    }

    fn encrypt_plaintext(plaintext: &[u8]) -> Vec<u8> {
        let encryptor = Encryptor::with_user_passphrase(SecretString::from(TEST_KEY.to_owned()));
        let mut encrypted = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut encrypted)
            .expect("create test encryption stream");
        writer.write_all(plaintext).expect("write test plaintext");
        writer.finish().expect("finish test encryption stream");
        encrypted
    }

    #[test]
    fn duplicate_secret_names_are_rejected() {
        let encrypted =
            encrypt_plaintext(br#"{"version":1,"secrets":{"token":"first","token":"second"}}"#);
        let error = EncryptedVault::from_bytes(&encrypted, TEST_KEY).expect_err("duplicate name");
        assert!(matches!(
            error,
            VaultError::InvalidDocument(message) if message.contains("duplicate secret name")
        ));
    }

    #[cfg(unix)]
    #[test]
    fn saved_vault_is_owner_readable_only() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_path("permissions");
        EncryptedVault::new()
            .save(&path, TEST_KEY)
            .expect("save test vault");
        let mode = fs::metadata(&path)
            .expect("read test vault metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_file(path).expect("remove test vault");
    }

    #[test]
    fn invalid_names_and_weak_keys_are_rejected() {
        let mut vault = EncryptedVault::new();
        assert!(matches!(
            vault.set("bad name", "value"),
            Err(VaultError::InvalidName(_))
        ));
        assert!(matches!(
            vault.save(temp_path("weak"), "short"),
            Err(VaultError::WeakPassphrase)
        ));
    }
}
