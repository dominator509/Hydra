//! reference/bridge/host.rs — Wasmtime component host with capability grants + fuel (EP-004 M2).
//! The hardest plumbing in the repo: component-model instantiation, async host imports,
//! grant enforcement at EVERY host call, deterministic fuel kill. INV-2: this is the
//! only crate that links wasmtime. Deps: wasmtime = { features = ["component-model","async"] }.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};

// Generated bindings from wit/hydra-bridge.wit live in crates/bridge-wit:
//   wasmtime::component::bindgen!({ world: "bridge", path: "../../wit", async: true });
// Below we reference the generated module as `bindings` — adjust the path when copying.
use crate::bindings::{self, hydra::bridge::host as host_if, hydra::bridge::types};

/// Per-adapter capability grant (SPEC-002 secret_grant table).
#[derive(Debug, Clone)]
pub struct Grant {
    pub adapter_id: String,
    pub origins: Vec<String>,        // exact scheme+host[:port] prefixes
    pub secret_names: Vec<String>,
    pub dsn_name: Option<String>,
    pub fuel: u64,                   // e.g. 5_000_000_000; exhaustion = trap = kill
}

impl Grant {
    fn origin_allowed(&self, url: &str) -> bool {
        self.origins.iter().any(|o| {
            url.starts_with(o)
                && url[o.len()..].chars().next().map_or(true, |c| c == '/' || c == '?')
        })
    }
}

/// Everything host functions may touch. NOTE: no raw DB pool for CDM here —
/// adapters cannot reach CDM; kv/secrets/egress only.
pub struct HostState {
    pub grant: Grant,
    pub kv: Box<dyn KvStore>,            // store::adapter_kv behind a trait
    pub secrets: Box<dyn SecretSource>,  // vault behind a trait; returns by NAME only
    pub egress: Box<dyn EgressClient>,   // fabric::egress proxy client
    pub sql: Option<Box<dyn ReplicaSql>>,
}

#[async_trait::async_trait]
pub trait KvStore: Send { async fn get(&self, k: &str) -> Option<String>; async fn set(&mut self, k: &str, v: &str); }
#[async_trait::async_trait]
pub trait SecretSource: Send { async fn get(&self, name: &str) -> Option<String>; }
#[async_trait::async_trait]
pub trait EgressClient: Send {
    async fn send(&self, method: &str, url: &str, headers: &[(String, String)], body: Option<Vec<u8>>)
        -> Result<(u16, Vec<(String, String)>, Vec<u8>)>;
}
#[async_trait::async_trait]
pub trait ReplicaSql: Send { async fn query_json(&self, q: &str, params: &[String]) -> Result<String>; }

pub struct BridgeHost { engine: Engine, linker: Linker<HostState> }

impl BridgeHost {
    pub fn new() -> Result<Self> {
        let mut cfg = Config::new();
        cfg.wasm_component_model(true);
        cfg.async_support(true);       // async host fns REQUIRE this before instantiate_async
        cfg.consume_fuel(true);        // deterministic CPU budget (Grant.fuel)
        let engine = Engine::new(&cfg)?;

        let mut linker: Linker<HostState> = Linker::new(&engine);
        // Wire the generated host interface to our impls:
        host_if::add_to_linker(&mut linker, |s: &mut HostState| s)?;
        Ok(Self { engine, linker })
    }

    pub async fn instantiate(&self, wasm: &[u8], state: HostState) -> Result<AdapterHandle> {
        let component = Component::new(&self.engine, wasm).context("component decode")?;
        let mut store = Store::new(&self.engine, state);
        store.set_fuel(store.data().grant.fuel)?; // trap-on-exhaust kills runaway adapters
        let bindings = bindings::Bridge::instantiate_async(&mut store, &component, &self.linker)
            .await.context("instantiate")?;
        Ok(AdapterHandle { store, bindings })
    }
}

pub struct AdapterHandle { store: Store<HostState>, bindings: bindings::Bridge }

impl AdapterHandle {
    pub async fn changes_since(&mut self, cursor: &str, limit: u32)
        -> Result<Result<types::ChangePage, types::BridgeError>>
    {
        self.bindings.hydra_bridge_adapter()
            .call_changes_since(&mut self.store, cursor, limit).await
    }
    // list/get/upsert/delete/describe/probe/introspect wrappers follow the same shape.
}

// ---------- Host import implementations: EVERY call re-checks the grant ----------

#[async_trait::async_trait]
impl host_if::Host for HostState {
    async fn http(&mut self, req: host_if::HttpRequest)
        -> Result<Result<host_if::HttpResponse, types::BridgeError>>
    {
        if !self.grant.origin_allowed(&req.url) {
            // Deny is a RESULT, not a trap: adapters must handle policy errors gracefully.
            return Ok(Err(types::BridgeError::Invalid(
                format!("origin not in grant: {}", redact_url(&req.url)))));
        }
        match self.egress.send(&req.method, &req.url, &req.headers, req.body).await {
            Ok((status, headers, body)) => Ok(Ok(host_if::HttpResponse { status, headers, body })),
            Err(e) => Ok(Err(types::BridgeError::Upstream(e.to_string()))),
        }
    }

    async fn secret(&mut self, name: String) -> Result<Result<String, types::BridgeError>> {
        if !self.grant.secret_names.iter().any(|n| n == &name) {
            return Ok(Err(types::BridgeError::Invalid(format!("secret not granted: {name}"))));
        }
        match self.secrets.get(&name).await {
            Some(v) => Ok(Ok(v)),
            None => Ok(Err(types::BridgeError::Invalid(format!("secret missing in vault: {name}")))),
        }
    }

    async fn kv_get(&mut self, key: String) -> Result<Option<String>> {
        Ok(self.kv.get(&namespaced(&self.grant.adapter_id, &key)).await)
    }
    async fn kv_set(&mut self, key: String, value: String) -> Result<()> {
        self.kv.set(&namespaced(&self.grant.adapter_id, &key), &value).await; Ok(())
    }

    async fn sql_query(&mut self, query: String, params: Vec<String>)
        -> Result<Result<String, types::BridgeError>>
    {
        let Some(sql) = self.sql.as_ref() else {
            return Ok(Err(types::BridgeError::Invalid("no DSN in grant".into())));
        };
        if !query.trim_start().to_ascii_lowercase().starts_with("select") {
            return Ok(Err(types::BridgeError::Invalid("read-only replica: SELECT only".into())));
        }
        match sql.query_json(&query, &params).await {
            Ok(j) => Ok(Ok(j)),
            Err(e) => Ok(Err(types::BridgeError::Upstream(e.to_string()))),
        }
    }

    async fn log(&mut self, level: String, msg: String) -> Result<()> {
        // Adapter logs are UNTRUSTED text: length-cap + strip control chars before tracing.
        let msg: String = msg.chars().filter(|c| !c.is_control()).take(2048).collect();
        match level.as_str() {
            "error" => tracing::error!(adapter = %self.grant.adapter_id, "{msg}"),
            "warn"  => tracing::warn!(adapter = %self.grant.adapter_id, "{msg}"),
            _       => tracing::info!(adapter = %self.grant.adapter_id, "{msg}"),
        }
        Ok(())
    }

    async fn now_ms(&mut self) -> Result<u64> {
        Ok(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64)
    }
}

fn namespaced(adapter: &str, key: &str) -> String { format!("{adapter}::{key}") }
fn redact_url(u: &str) -> String { u.split('?').next().unwrap_or(u).to_string() }

// Fuel-kill behavior worth knowing when you copy this file:
// exhaustion raises Trap::OutOfFuel from the in-flight call_* — map it to
// bridge_errors_total{variant="fuel"} and PARK the adapter (SPEC-006 policy),
// never auto-refuel in a loop.
