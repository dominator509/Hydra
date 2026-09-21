use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use store::AdapterKvRepo;
use thiserror::Error;
use uuid::Uuid;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::WasiCtxBuilder;

use crate::{
    bindings::hydra::bridge::types, BridgeHost, EgressClient, Grant, HostState, SecretSource,
    StaticSecretSource, TenantStoreKvStore,
};

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("invalid adapter component reference: {0}")]
    InvalidReference(String),
    #[error("adapter component is unavailable: {0}")]
    ArtifactUnavailable(String),
    #[error("adapter component digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },
    #[error("invalid adapter grant: {0}")]
    InvalidGrant(String),
    #[error("adapter host failed: {0}")]
    Host(String),
    #[error("adapter probe failed: {0}")]
    Probe(String),
    #[error("adapter synchronization failed: {0}")]
    Sync(String),
    #[error("adapter conformance failed: {0}")]
    Conformance(String),
}

pub const FULL_RELIST_MAX_PAGES: usize = 256;
pub const FULL_RELIST_MAX_RECORDS: usize = 10_000;
pub const FULL_RELIST_MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ComponentRoot {
    canonical_root: PathBuf,
}

impl ComponentRoot {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, LifecycleError> {
        let root = root.as_ref();
        let metadata = std::fs::metadata(root).map_err(|error| {
            LifecycleError::ArtifactUnavailable(format!("{}: {error}", root.display()))
        })?;
        if !metadata.is_dir() {
            return Err(LifecycleError::ArtifactUnavailable(format!(
                "adapter root is not a directory: {}",
                root.display()
            )));
        }
        let canonical_root = std::fs::canonicalize(root).map_err(|error| {
            LifecycleError::ArtifactUnavailable(format!("{}: {error}", root.display()))
        })?;
        Ok(Self { canonical_root })
    }

    pub fn load(
        &self,
        component_ref: &str,
        expected_sha256: Option<&str>,
    ) -> Result<ComponentArtifact, LifecycleError> {
        let path = self.resolve(component_ref)?;
        let bytes = std::fs::read(&path).map_err(|error| {
            LifecycleError::ArtifactUnavailable(format!("{}: {error}", path.display()))
        })?;
        let sha256 = sha256_hex(&bytes);
        if let Some(expected) = expected_sha256 {
            if expected != sha256 {
                return Err(LifecycleError::DigestMismatch {
                    expected: expected.to_owned(),
                    actual: sha256,
                });
            }
        }
        Ok(ComponentArtifact {
            component_ref: component_ref.to_owned(),
            path,
            sha256,
            bytes,
        })
    }

    fn resolve(&self, component_ref: &str) -> Result<PathBuf, LifecycleError> {
        let relative = Path::new(component_ref);
        if component_ref.trim().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            || relative
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("wasm")
        {
            return Err(LifecycleError::InvalidReference(component_ref.to_owned()));
        }

        let candidate = self.canonical_root.join(relative);
        let metadata = std::fs::symlink_metadata(&candidate).map_err(|error| {
            LifecycleError::ArtifactUnavailable(format!("{}: {error}", candidate.display()))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(LifecycleError::InvalidReference(format!(
                "symlink is not allowed: {component_ref}"
            )));
        }
        let canonical = std::fs::canonicalize(&candidate).map_err(|error| {
            LifecycleError::ArtifactUnavailable(format!("{}: {error}", candidate.display()))
        })?;
        if !canonical.starts_with(&self.canonical_root) {
            return Err(LifecycleError::InvalidReference(component_ref.to_owned()));
        }
        let file_metadata = std::fs::metadata(&canonical).map_err(|error| {
            LifecycleError::ArtifactUnavailable(format!("{}: {error}", canonical.display()))
        })?;
        if !file_metadata.is_file() {
            return Err(LifecycleError::ArtifactUnavailable(format!(
                "component is not a file: {}",
                canonical.display()
            )));
        }
        Ok(canonical)
    }
}

#[derive(Debug, Clone)]
pub struct ComponentArtifact {
    pub component_ref: String,
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeCapabilities {
    pub read: bool,
    pub write: bool,
    pub incremental_sync: bool,
    pub etags: bool,
    pub server_side_query: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeDescriptor {
    pub name: String,
    pub version: String,
    pub kinds: Vec<String>,
    pub capabilities: BridgeCapabilities,
}

#[derive(Debug, Clone)]
pub struct ProbeRequest<'a> {
    pub tenant_id: Uuid,
    pub adapter_id: &'a str,
    pub component_ref: &'a str,
    pub expected_sha256: Option<&'a str>,
    pub grant: Grant,
    pub config_json: &'a str,
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub artifact: ComponentArtifact,
    pub descriptor: BridgeDescriptor,
    pub fuel_remaining: u64,
}

#[derive(Debug, Clone)]
pub struct SyncRequest<'a> {
    pub tenant_id: Uuid,
    pub adapter_id: &'a str,
    pub component_ref: &'a str,
    pub expected_sha256: Option<&'a str>,
    pub grant: Grant,
    pub config_json: &'a str,
    pub cursor: &'a str,
    pub limit: u32,
}

#[derive(Debug, Clone)]
pub struct SyncPage {
    pub artifact: ComponentArtifact,
    pub changes: types::ChangePage,
    pub fuel_remaining: u64,
}

#[derive(Debug, Clone)]
pub struct FullRelistRequest<'a> {
    pub tenant_id: Uuid,
    pub adapter_id: &'a str,
    pub component_ref: &'a str,
    pub expected_sha256: Option<&'a str>,
    pub grant: Grant,
    pub config_json: &'a str,
    pub kind: &'a str,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FullRelistRecord {
    pub kind: String,
    pub id: String,
    pub data: String,
}

#[derive(Debug, Clone)]
pub struct FullRelistResult {
    pub artifact: ComponentArtifact,
    pub records: Vec<FullRelistRecord>,
    pub page_count: u32,
    pub total_bytes: usize,
    pub fuel_remaining: u64,
}

#[derive(Debug, Clone)]
pub struct ConformanceRequest<'a> {
    pub tenant_id: Uuid,
    pub adapter_id: &'a str,
    pub component_ref: &'a str,
    pub expected_sha256: Option<&'a str>,
    pub grant: Grant,
    pub config_json: &'a str,
    pub kind: Option<&'a str>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConformanceResult {
    pub artifact_sha256: String,
    pub descriptor: BridgeDescriptor,
    pub checked_kind: String,
    pub schema_field_count: u32,
    pub listed_record_count: u32,
    pub changed_record_count: u32,
    pub incremental_checked: bool,
    pub fuel_remaining: u64,
    pub report: String,
}

pub struct BridgeLifecycle {
    host: Arc<BridgeHost>,
    root: ComponentRoot,
    adapter_kv: AdapterKvRepo,
    secrets: Arc<dyn SecretSource>,
    egress: Arc<dyn EgressClient>,
}

impl BridgeLifecycle {
    pub fn new(
        host: Arc<BridgeHost>,
        root: ComponentRoot,
        adapter_kv: AdapterKvRepo,
        secrets: Arc<dyn SecretSource>,
        egress: Arc<dyn EgressClient>,
    ) -> Self {
        Self {
            host,
            root,
            adapter_kv,
            secrets,
            egress,
        }
    }

    pub fn disabled_for_tests(
        host: Arc<BridgeHost>,
        root: ComponentRoot,
        adapter_kv: AdapterKvRepo,
    ) -> Self {
        Self::new(
            host,
            root,
            adapter_kv,
            Arc::new(StaticSecretSource::default()),
            Arc::new(DenyEgressClient),
        )
    }

    /// Load a configured component without instantiating it.
    ///
    /// The kernel uses this first to persist the exact artifact digest before
    /// probing. A later probe must supply that digest again, so a changed
    /// component cannot be activated under an existing adapter identity.
    pub fn load_artifact(
        &self,
        component_ref: &str,
        expected_sha256: Option<&str>,
    ) -> Result<ComponentArtifact, LifecycleError> {
        self.root.load(component_ref, expected_sha256)
    }

    pub async fn probe(&self, request: ProbeRequest<'_>) -> Result<ProbeResult, LifecycleError> {
        validate_grant(request.adapter_id, &request.grant)?;
        let artifact = self
            .root
            .load(request.component_ref, request.expected_sha256)?;
        let state = self.host_state(request.tenant_id, request.adapter_id, request.grant);
        let mut handle = self
            .host
            .instantiate(&artifact.bytes, state)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        let declared = handle
            .describe()
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        let probed = handle
            .probe(request.config_json)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?
            .map_err(|error| LifecycleError::Probe(sanitize_error(format!("{error:?}"))))?;
        let declared = descriptor_from_wit(declared);
        let descriptor = descriptor_from_wit(probed);
        if declared.name != descriptor.name || declared.version != descriptor.version {
            return Err(LifecycleError::Probe(
                "probe descriptor does not match describe descriptor".to_owned(),
            ));
        }
        let fuel_remaining = handle
            .fuel_remaining()
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        Ok(ProbeResult {
            artifact,
            descriptor,
            fuel_remaining,
        })
    }

    pub async fn sync_page(&self, request: SyncRequest<'_>) -> Result<SyncPage, LifecycleError> {
        validate_grant(request.adapter_id, &request.grant)?;
        if request.limit == 0 || request.limit > 100 || request.cursor.len() > 2048 {
            return Err(LifecycleError::Sync(
                "sync cursor or limit is outside the host bound".to_owned(),
            ));
        }
        if request.cursor.chars().any(char::is_control) {
            return Err(LifecycleError::Sync(
                "sync cursor contains a control character".to_owned(),
            ));
        }
        let artifact = self
            .root
            .load(request.component_ref, request.expected_sha256)?;
        let state = self.host_state(request.tenant_id, request.adapter_id, request.grant);
        let mut handle = self
            .host
            .instantiate(&artifact.bytes, state)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        handle
            .probe(request.config_json)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?
            .map_err(|error| LifecycleError::Sync(sanitize_error(format!("{error:?}"))))?;
        let changes = handle
            .changes_since(request.cursor, request.limit)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?
            .map_err(|error| LifecycleError::Sync(sanitize_error(format!("{error:?}"))))?;
        if changes.next_cursor.len() > 2048 || changes.next_cursor.chars().any(char::is_control) {
            return Err(LifecycleError::Sync(
                "adapter returned an invalid next cursor".to_owned(),
            ));
        }
        let fuel_remaining = handle
            .fuel_remaining()
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        Ok(SyncPage {
            artifact,
            changes,
            fuel_remaining,
        })
    }

    /// Read a complete adapter snapshot through the WIT list export.
    ///
    /// The result is intentionally bounded and owned so the caller can hand
    /// it to Store for one atomic diff without exposing raw records outside
    /// the governed execution path.
    pub async fn full_relist(
        &self,
        request: FullRelistRequest<'_>,
    ) -> Result<FullRelistResult, LifecycleError> {
        validate_grant(request.adapter_id, &request.grant)?;
        if request.limit == 0 || request.limit > 100 || !valid_text(request.kind, 128) {
            return Err(LifecycleError::Sync(
                "full relist kind or limit is outside the host bound".to_owned(),
            ));
        }

        let artifact = self
            .root
            .load(request.component_ref, request.expected_sha256)?;
        let state = self.host_state(request.tenant_id, request.adapter_id, request.grant);
        let mut handle = self
            .host
            .instantiate(&artifact.bytes, state)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        let declared = handle
            .describe()
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        let probed = handle
            .probe(request.config_json)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?
            .map_err(|error| LifecycleError::Sync(sanitize_error(format!("{error:?}"))))?;
        let declared = descriptor_from_wit(declared);
        let descriptor = descriptor_from_wit(probed);
        validate_descriptor(&declared).map_err(|error| LifecycleError::Sync(error.to_string()))?;
        validate_descriptor(&descriptor)
            .map_err(|error| LifecycleError::Sync(error.to_string()))?;
        if declared.name != descriptor.name || declared.version != descriptor.version {
            return Err(LifecycleError::Sync(
                "probe descriptor does not match describe descriptor".to_owned(),
            ));
        }
        if !descriptor.capabilities.read
            || !descriptor.kinds.iter().any(|kind| kind == request.kind)
        {
            return Err(LifecycleError::Sync(
                "adapter does not advertise read capability for this kind".to_owned(),
            ));
        }

        let mut cursor: Option<String> = None;
        let mut seen_cursors = std::collections::BTreeSet::new();
        let mut seen_identities = std::collections::BTreeSet::new();
        let mut records = Vec::new();
        let mut total_bytes = 0usize;
        let mut page_count = 0usize;

        loop {
            if page_count >= FULL_RELIST_MAX_PAGES {
                return Err(LifecycleError::Sync(
                    "full relist exceeded the page bound".to_owned(),
                ));
            }
            let page = handle
                .list(request.kind, cursor.as_deref(), request.limit)
                .await
                .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?
                .map_err(|error| LifecycleError::Sync(sanitize_error(format!("{error:?}"))))?;
            records.extend(accept_full_relist_page(
                &page,
                request.kind,
                request.limit,
                &mut seen_identities,
                records.len(),
                &mut total_bytes,
            )?);
            page_count += 1;

            let Some(next_cursor) = next_full_relist_cursor(page.next_cursor, &mut seen_cursors)?
            else {
                break;
            };
            cursor = Some(next_cursor);
        }

        let fuel_remaining = handle
            .fuel_remaining()
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        Ok(FullRelistResult {
            artifact,
            records,
            page_count: page_count as u32,
            total_bytes,
            fuel_remaining,
        })
    }

    /// Exercise only read-side WIT exports and return bounded metadata.
    /// Conformance deliberately never calls adapter mutation exports or Store.
    pub async fn conformance(
        &self,
        request: ConformanceRequest<'_>,
    ) -> Result<ConformanceResult, LifecycleError> {
        validate_grant(request.adapter_id, &request.grant)?;
        if request.limit == 0 || request.limit > 100 {
            return Err(LifecycleError::Conformance(
                "conformance limit is outside the host bound".to_owned(),
            ));
        }
        if request.kind.is_some_and(|kind| !valid_text(kind, 128)) {
            return Err(LifecycleError::Conformance(
                "conformance kind is invalid".to_owned(),
            ));
        }

        let artifact = self
            .root
            .load(request.component_ref, request.expected_sha256)?;
        let state = self.host_state(request.tenant_id, request.adapter_id, request.grant);
        let mut handle = self
            .host
            .instantiate(&artifact.bytes, state)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        let declared = handle
            .describe()
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        let probed = handle
            .probe(request.config_json)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?
            .map_err(|error| LifecycleError::Conformance(sanitize_error(format!("{error:?}"))))?;
        let declared = descriptor_from_wit(declared);
        let descriptor = descriptor_from_wit(probed);
        validate_descriptor(&declared)?;
        validate_descriptor(&descriptor)?;
        if declared.name != descriptor.name || declared.version != descriptor.version {
            return Err(LifecycleError::Conformance(
                "probe descriptor does not match describe descriptor".to_owned(),
            ));
        }
        if descriptor.capabilities.incremental_sync && !descriptor.capabilities.read {
            return Err(LifecycleError::Conformance(
                "incremental-sync requires read capability".to_owned(),
            ));
        }

        let checked_kind = request
            .kind
            .map(str::to_owned)
            .or_else(|| descriptor.kinds.first().cloned())
            .ok_or_else(|| {
                LifecycleError::Conformance("descriptor declares no CRM kinds".to_owned())
            })?;
        if !descriptor.kinds.iter().any(|kind| kind == &checked_kind) {
            return Err(LifecycleError::Conformance(
                "requested kind is not declared by the adapter".to_owned(),
            ));
        }
        let schema = handle
            .introspect_schema(&checked_kind)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?
            .map_err(|error| LifecycleError::Conformance(sanitize_error(format!("{error:?}"))))?;
        validate_schema(&schema)?;
        let page = handle
            .list(&checked_kind, None, request.limit)
            .await
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?
            .map_err(|error| LifecycleError::Conformance(sanitize_error(format!("{error:?}"))))?;
        validate_page(&page, &checked_kind, request.limit)?;

        let incremental_checked = descriptor.capabilities.incremental_sync;
        let changed_record_count = if incremental_checked {
            let changes = handle
                .changes_since("", request.limit)
                .await
                .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?
                .map_err(|error| {
                    LifecycleError::Conformance(sanitize_error(format!("{error:?}")))
                })?;
            validate_change_page(&changes, &descriptor, request.limit)?;
            changes.changes.len() as u32
        } else {
            0
        };
        let fuel_remaining = handle
            .fuel_remaining()
            .map_err(|error| LifecycleError::Host(sanitize_error(error.to_string())))?;
        let incremental_report = if incremental_checked {
            "changes-since:checked"
        } else {
            "changes-since:not-declared"
        };
        Ok(ConformanceResult {
            artifact_sha256: artifact.sha256,
            descriptor,
            checked_kind,
            schema_field_count: schema.len() as u32,
            listed_record_count: page.records.len() as u32,
            changed_record_count,
            incremental_checked,
            fuel_remaining,
            report: format!(
                "describe:pass; probe:pass; schema:pass; list:pass; {incremental_report}"
            ),
        })
    }

    fn host_state(&self, tenant_id: Uuid, adapter_id: &str, grant: Grant) -> HostState {
        HostState {
            grant,
            kv: Box::new(TenantStoreKvStore::new(
                self.adapter_kv.clone(),
                tenant_id,
                adapter_id.to_owned(),
            )),
            secrets: Box::new(SharedSecretSource(self.secrets.clone())),
            egress: Box::new(SharedEgressClient(self.egress.clone())),
            sql: None,
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
        }
    }
}

fn validate_grant(adapter_id: &str, grant: &Grant) -> Result<(), LifecycleError> {
    if adapter_id.trim().is_empty() || grant.adapter_id != adapter_id {
        return Err(LifecycleError::InvalidGrant(
            "grant adapter identity does not match request".to_owned(),
        ));
    }
    if grant.fuel == 0 {
        return Err(LifecycleError::InvalidGrant(
            "grant fuel must be greater than zero".to_owned(),
        ));
    }
    if grant
        .origins
        .iter()
        .chain(grant.secret_names.iter())
        .chain(grant.dsn_name.iter())
        .any(|value| value.is_empty() || value.chars().any(char::is_control))
    {
        return Err(LifecycleError::InvalidGrant(
            "grant contains an empty or control-character value".to_owned(),
        ));
    }
    Ok(())
}

fn descriptor_from_wit(
    descriptor: crate::bindings::hydra::bridge::types::Descriptor,
) -> BridgeDescriptor {
    BridgeDescriptor {
        name: descriptor.name,
        version: descriptor.version,
        kinds: descriptor.kinds,
        capabilities: BridgeCapabilities {
            read: descriptor.caps.read,
            write: descriptor.caps.write,
            incremental_sync: descriptor.caps.incremental_sync,
            etags: descriptor.caps.etags,
            server_side_query: descriptor.caps.server_side_query,
        },
    }
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
}

fn validate_descriptor(descriptor: &BridgeDescriptor) -> Result<(), LifecycleError> {
    if !valid_text(&descriptor.name, 128) || !valid_text(&descriptor.version, 64) {
        return Err(LifecycleError::Conformance(
            "descriptor name or version is invalid".to_owned(),
        ));
    }
    if descriptor.kinds.is_empty() || descriptor.kinds.len() > 64 {
        return Err(LifecycleError::Conformance(
            "descriptor kinds must contain 1..=64 entries".to_owned(),
        ));
    }
    let mut kinds = std::collections::BTreeSet::new();
    for kind in &descriptor.kinds {
        if !valid_text(kind, 128) || !kinds.insert(kind) {
            return Err(LifecycleError::Conformance(
                "descriptor kinds must be unique bounded names".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_schema(schema: &[types::FieldSchema]) -> Result<(), LifecycleError> {
    if schema.len() > 256 {
        return Err(LifecycleError::Conformance(
            "schema contains too many fields".to_owned(),
        ));
    }
    let mut names = std::collections::BTreeSet::new();
    for field in schema {
        if !valid_text(&field.name, 128)
            || !valid_text(&field.ty, 64)
            || !names.insert(&field.name)
            || field.enum_values.len() > 128
            || field
                .enum_values
                .iter()
                .any(|value| !valid_text(value, 128))
        {
            return Err(LifecycleError::Conformance(
                "schema contains an invalid or duplicate field".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_page(
    page: &types::Page,
    expected_kind: &str,
    limit: u32,
) -> Result<(), LifecycleError> {
    if page.records.len() > limit as usize {
        return Err(LifecycleError::Conformance(
            "adapter list page exceeds requested limit".to_owned(),
        ));
    }
    if page
        .next_cursor
        .as_deref()
        .is_some_and(|cursor| cursor.len() > 2048 || cursor.chars().any(char::is_control))
    {
        return Err(LifecycleError::Conformance(
            "adapter list cursor is invalid".to_owned(),
        ));
    }
    let mut identities = std::collections::BTreeSet::new();
    for record in &page.records {
        if record.kind != expected_kind
            || !valid_text(&record.kind, 128)
            || !valid_text(&record.id, 256)
            || record.data.len() > 256 * 1024
            || !serde_json::from_str::<serde_json::Value>(&record.data)
                .ok()
                .is_some_and(|value| value.is_object())
            || !identities.insert((&record.kind, &record.id))
        {
            return Err(LifecycleError::Conformance(
                "adapter list page contains invalid or duplicate record identity".to_owned(),
            ));
        }
    }
    Ok(())
}

fn accept_full_relist_page(
    page: &types::Page,
    expected_kind: &str,
    limit: u32,
    seen_identities: &mut std::collections::BTreeSet<(String, String)>,
    existing_records: usize,
    total_bytes: &mut usize,
) -> Result<Vec<FullRelistRecord>, LifecycleError> {
    validate_page(page, expected_kind, limit)
        .map_err(|error| LifecycleError::Sync(error.to_string()))?;
    if existing_records
        .checked_add(page.records.len())
        .is_none_or(|count| count > FULL_RELIST_MAX_RECORDS)
    {
        return Err(LifecycleError::Sync(
            "full relist exceeded the record bound".to_owned(),
        ));
    }

    let mut records = Vec::with_capacity(page.records.len());
    for record in &page.records {
        let identity = (record.kind.clone(), record.id.clone());
        if !seen_identities.insert(identity) {
            return Err(LifecycleError::Sync(
                "full relist contains a duplicate record identity".to_owned(),
            ));
        }
        let next_bytes = record
            .kind
            .len()
            .checked_add(record.id.len())
            .and_then(|bytes| bytes.checked_add(record.data.len()))
            .and_then(|bytes| total_bytes.checked_add(bytes))
            .ok_or_else(|| LifecycleError::Sync("full relist byte bound overflowed".to_owned()))?;
        if next_bytes > FULL_RELIST_MAX_BYTES {
            return Err(LifecycleError::Sync(
                "full relist exceeded the byte bound".to_owned(),
            ));
        }
        *total_bytes = next_bytes;
        records.push(FullRelistRecord {
            kind: record.kind.clone(),
            id: record.id.clone(),
            data: record.data.clone(),
        });
    }
    Ok(records)
}

fn next_full_relist_cursor(
    cursor: Option<String>,
    seen_cursors: &mut std::collections::BTreeSet<String>,
) -> Result<Option<String>, LifecycleError> {
    match cursor {
        None => Ok(None),
        Some(cursor) if cursor.is_empty() || !seen_cursors.insert(cursor.clone()) => Err(
            LifecycleError::Sync("full relist returned a repeated or empty cursor".to_owned()),
        ),
        Some(cursor) => Ok(Some(cursor)),
    }
}

fn validate_change_page(
    page: &types::ChangePage,
    descriptor: &BridgeDescriptor,
    limit: u32,
) -> Result<(), LifecycleError> {
    if page.changes.len() > limit as usize
        || page.next_cursor.len() > 2048
        || page.next_cursor.chars().any(char::is_control)
    {
        return Err(LifecycleError::Conformance(
            "adapter change page exceeds bounds".to_owned(),
        ));
    }
    let mut identities = std::collections::BTreeSet::new();
    for change in &page.changes {
        let record = &change.rec;
        if !descriptor.kinds.iter().any(|kind| kind == &record.kind)
            || !valid_text(&record.kind, 128)
            || !valid_text(&record.id, 256)
            || record.data.len() > 256 * 1024
            || !serde_json::from_str::<serde_json::Value>(&record.data)
                .ok()
                .is_some_and(|value| value.is_object())
            || !identities.insert((&record.kind, &record.id))
        {
            return Err(LifecycleError::Conformance(
                "adapter change page contains invalid or duplicate identity".to_owned(),
            ));
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn sanitize_error(value: String) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(512)
        .collect()
}

struct SharedSecretSource(Arc<dyn SecretSource>);

#[async_trait]
impl SecretSource for SharedSecretSource {
    async fn get(&self, name: &str) -> anyhow::Result<Option<String>> {
        self.0.get(name).await
    }
}

struct SharedEgressClient(Arc<dyn EgressClient>);

#[async_trait]
impl EgressClient for SharedEgressClient {
    async fn send(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<Vec<u8>>,
    ) -> anyhow::Result<(u16, Vec<(String, String)>, Vec<u8>)> {
        self.0.send(method, url, headers, body).await
    }
}

#[derive(Default)]
pub struct DenyEgressClient;

#[async_trait]
impl EgressClient for DenyEgressClient {
    async fn send(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<Vec<u8>>,
    ) -> anyhow::Result<(u16, Vec<(String, String)>, Vec<u8>)> {
        Err(anyhow::anyhow!("bridge egress is not configured"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, data: &str) -> types::RawRecord {
        types::RawRecord {
            kind: "party".to_owned(),
            id: id.to_owned(),
            etag: None,
            data: data.to_owned(),
        }
    }

    #[test]
    fn conformance_rejects_malformed_record_json() {
        let page = types::Page {
            records: vec![record("party-1", "[]")],
            next_cursor: None,
        };
        let error = validate_page(&page, "party", 25).expect_err("array data must be rejected");
        assert!(error
            .to_string()
            .contains("invalid or duplicate record identity"));
    }

    #[test]
    fn conformance_rejects_duplicate_record_identity() {
        let page = types::Page {
            records: vec![record("party-1", "{}"), record("party-1", "{}")],
            next_cursor: None,
        };
        let error = validate_page(&page, "party", 25)
            .expect_err("duplicate record identities must be rejected");
        assert!(error
            .to_string()
            .contains("invalid or duplicate record identity"));
    }

    #[test]
    fn conformance_rejects_invalid_cursor() {
        let page = types::Page {
            records: Vec::new(),
            next_cursor: Some("cursor\nwith-control".to_owned()),
        };
        let error = validate_page(&page, "party", 25).expect_err("control cursor must be rejected");
        assert!(error.to_string().contains("list cursor is invalid"));
    }

    #[test]
    fn full_relist_accumulates_distinct_pages_with_bounds() {
        let first = types::Page {
            records: vec![record("party-1", r#"{"display_name":"Ada"}"#)],
            next_cursor: Some("page-2".to_owned()),
        };
        let second = types::Page {
            records: vec![record("party-2", r#"{"display_name":"Grace"}"#)],
            next_cursor: None,
        };
        let mut identities = std::collections::BTreeSet::new();
        let mut bytes = 0;
        let first_records =
            accept_full_relist_page(&first, "party", 25, &mut identities, 0, &mut bytes)
                .expect("first page should pass");
        let second_records = accept_full_relist_page(
            &second,
            "party",
            25,
            &mut identities,
            first_records.len(),
            &mut bytes,
        )
        .expect("second page should pass");
        assert_eq!(first_records.len() + second_records.len(), 2);
        assert!(bytes > 0);
    }

    #[test]
    fn full_relist_rejects_repeated_cursor_and_cross_page_duplicate() {
        let mut cursors = std::collections::BTreeSet::new();
        assert!(next_full_relist_cursor(Some("page-2".to_owned()), &mut cursors).is_ok());
        assert!(next_full_relist_cursor(Some("page-2".to_owned()), &mut cursors).is_err());

        let page = types::Page {
            records: vec![record("party-1", "{}")],
            next_cursor: None,
        };
        let mut identities = std::collections::BTreeSet::new();
        let mut bytes = 0;
        accept_full_relist_page(&page, "party", 25, &mut identities, 0, &mut bytes)
            .expect("first identity should pass");
        assert!(
            accept_full_relist_page(&page, "party", 25, &mut identities, 1, &mut bytes).is_err()
        );
    }

    #[test]
    fn full_relist_rejects_byte_bound() {
        let page = types::Page {
            records: vec![record("party-1", "{}")],
            next_cursor: None,
        };
        let mut identities = std::collections::BTreeSet::new();
        let mut bytes = FULL_RELIST_MAX_BYTES;
        let error = accept_full_relist_page(&page, "party", 25, &mut identities, 0, &mut bytes)
            .expect_err("byte bound must fail closed");
        assert!(error.to_string().contains("byte bound"));
    }
}
