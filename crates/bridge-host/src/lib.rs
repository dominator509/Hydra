//! layer L3 bridge-host: Wasmtime component host, grants, and adapter loading.

pub mod bindings {
    wasmtime::component::bindgen!({
        world: "bridge",
        path: "../../wit",
        imports: {
            default: async | trappable,
        },
        exports: {
            default: async,
        },
    });
}

pub mod grants;
pub mod host;
pub mod lifecycle;
pub mod loader;
pub mod vault;

pub use grants::{Grant, GrantTable};
pub use host::{
    AdapterHandle, BridgeHost, EgressClient, HostState, KvStore, ReplicaSql, ReqwestEgressClient,
    SecretSource, StaticSecretSource, StoreKvStore, TenantStoreKvStore,
};
pub use lifecycle::{
    BridgeCapabilities, BridgeDescriptor, BridgeLifecycle, ComponentArtifact, ComponentRoot,
    ConformanceRequest, ConformanceResult, DenyEgressClient, FullRelistRecord, FullRelistRequest,
    FullRelistResult, LifecycleError, ProbeRequest, ProbeResult, SyncPage, SyncRequest,
    FULL_RELIST_MAX_BYTES, FULL_RELIST_MAX_PAGES, FULL_RELIST_MAX_RECORDS,
};
pub use loader::{default_adapter_path, instantiate_file, load_component_bytes};
pub use vault::{EncryptedVault, VaultError, VaultSecretSource, DEFAULT_VAULT_PATH};
