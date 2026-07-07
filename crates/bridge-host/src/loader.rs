use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::host::{AdapterHandle, BridgeHost, HostState};

pub fn load_component_bytes(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    std::fs::read(path)
        .with_context(|| format!("failed to read adapter component {}", path.display()))
}

pub fn default_adapter_path(adapter_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../adapters")
        .join(format!("{adapter_name}.wasm"))
}

pub async fn instantiate_file(
    host: &BridgeHost,
    path: impl AsRef<Path>,
    state: HostState,
) -> Result<AdapterHandle> {
    let wasm = load_component_bytes(path)?;
    host.instantiate(&wasm, state).await
}
