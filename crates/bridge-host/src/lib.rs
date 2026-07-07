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
pub mod loader;

pub use grants::{Grant, GrantTable};
pub use host::{
    AdapterHandle, BridgeHost, EgressClient, HostState, KvStore, ReplicaSql, ReqwestEgressClient,
    SecretSource, StaticSecretSource, StoreKvStore,
};
pub use loader::{default_adapter_path, instantiate_file, load_component_bytes};
