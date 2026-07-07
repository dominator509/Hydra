use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};
use store::AdapterKvRepo;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::bindings::{self, hydra::bridge::host as host_if, hydra::bridge::types};
use crate::grants::Grant;

pub struct HostState {
    pub grant: Grant,
    pub kv: Box<dyn KvStore>,
    pub secrets: Box<dyn SecretSource>,
    pub egress: Box<dyn EgressClient>,
    pub sql: Option<Box<dyn ReplicaSql>>,
    pub wasi: WasiCtx,
    pub table: ResourceTable,
}

impl types::Host for HostState {}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

#[async_trait]
pub trait KvStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<String>>;
    async fn set(&self, key: &str, value: &str) -> Result<()>;
}

#[async_trait]
pub trait SecretSource: Send + Sync {
    async fn get(&self, name: &str) -> Result<Option<String>>;
}

#[async_trait]
pub trait EgressClient: Send + Sync {
    async fn send(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<(String, String)>, Vec<u8>)>;
}

#[async_trait]
pub trait ReplicaSql: Send + Sync {
    async fn query_json(&self, query: &str, params: &[String]) -> Result<String>;
}

#[derive(Clone)]
pub struct StoreKvStore {
    adapter_id: String,
    repo: AdapterKvRepo,
}

impl StoreKvStore {
    pub fn new(repo: AdapterKvRepo, adapter_id: impl Into<String>) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            repo,
        }
    }
}

#[async_trait]
impl KvStore for StoreKvStore {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        self.repo
            .get(&self.adapter_id, key)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn set(&self, key: &str, value: &str) -> Result<()> {
        self.repo
            .set(&self.adapter_id, key, value)
            .await
            .map_err(anyhow::Error::from)
    }
}

#[derive(Clone, Default)]
pub struct StaticSecretSource {
    values: HashMap<String, String>,
}

impl StaticSecretSource {
    pub fn new(values: HashMap<String, String>) -> Self {
        Self { values }
    }
}

#[async_trait]
impl SecretSource for StaticSecretSource {
    async fn get(&self, name: &str) -> Result<Option<String>> {
        Ok(self.values.get(name).cloned())
    }
}

#[derive(Clone)]
pub struct ReqwestEgressClient {
    client: reqwest::Client,
}

impl ReqwestEgressClient {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl EgressClient for ReqwestEgressClient {
    async fn send(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<Vec<u8>>,
    ) -> Result<(u16, Vec<(String, String)>, Vec<u8>)> {
        let method: reqwest::Method = method.parse().context("invalid HTTP method")?;
        let mut request = self.client.request(method, url);

        for (name, value) in headers {
            let header_name = HeaderName::try_from(name.as_str()).context("invalid header name")?;
            let header_value =
                HeaderValue::try_from(value.as_str()).context("invalid header value")?;
            request = request.header(header_name, header_value);
        }

        if let Some(body) = body {
            request = request.body(body);
        }

        let response = request.send().await.context("egress request failed")?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    value.to_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        let body = response.bytes().await.context("egress body read failed")?;
        Ok((status, headers, body.to_vec()))
    }
}

pub struct BridgeHost {
    engine: Engine,
    linker: Linker<HostState>,
}

impl BridgeHost {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.async_support(true);
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        bindings::Bridge::add_to_linker::<HostState, wasmtime::component::HasSelf<HostState>>(
            &mut linker,
            |state: &mut HostState| state,
        )?;

        Ok(Self { engine, linker })
    }

    pub async fn instantiate(&self, wasm: &[u8], state: HostState) -> Result<AdapterHandle> {
        let component = Component::new(&self.engine, wasm).context("component decode")?;
        self.instantiate_component(&component, state).await
    }

    pub async fn instantiate_component(
        &self,
        component: &Component,
        state: HostState,
    ) -> Result<AdapterHandle> {
        let mut store = Store::new(&self.engine, state);
        store.set_fuel(store.data().grant.fuel)?;
        let bindings = bindings::Bridge::instantiate_async(&mut store, component, &self.linker)
            .await
            .context("instantiate")?;
        Ok(AdapterHandle { store, bindings })
    }
}

pub struct AdapterHandle {
    store: Store<HostState>,
    bindings: bindings::Bridge,
}

impl AdapterHandle {
    pub async fn describe(&mut self) -> Result<types::Descriptor> {
        self.bindings
            .hydra_bridge_adapter()
            .call_describe(&mut self.store)
            .await
            .context("call describe")
    }

    pub async fn probe(
        &mut self,
        config_json: &str,
    ) -> Result<Result<types::Descriptor, types::BridgeError>> {
        self.bindings
            .hydra_bridge_adapter()
            .call_probe(&mut self.store, config_json)
            .await
            .context("call probe")
    }

    pub async fn introspect_schema(
        &mut self,
        kind: &str,
    ) -> Result<Result<Vec<types::FieldSchema>, types::BridgeError>> {
        self.bindings
            .hydra_bridge_adapter()
            .call_introspect_schema(&mut self.store, kind)
            .await
            .context("call introspect-schema")
    }

    pub async fn list(
        &mut self,
        kind: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Result<types::Page, types::BridgeError>> {
        self.bindings
            .hydra_bridge_adapter()
            .call_list(&mut self.store, kind, cursor, limit)
            .await
            .context("call list")
    }

    pub async fn get(
        &mut self,
        kind: &str,
        id: &str,
    ) -> Result<Result<types::RawRecord, types::BridgeError>> {
        self.bindings
            .hydra_bridge_adapter()
            .call_get(&mut self.store, kind, id)
            .await
            .context("call get")
    }

    pub async fn upsert(
        &mut self,
        record: types::RawRecord,
    ) -> Result<Result<types::RawRecord, types::BridgeError>> {
        self.bindings
            .hydra_bridge_adapter()
            .call_upsert(&mut self.store, &record)
            .await
            .context("call upsert")
    }

    pub async fn delete(&mut self, kind: &str, id: &str) -> Result<Result<(), types::BridgeError>> {
        self.bindings
            .hydra_bridge_adapter()
            .call_delete(&mut self.store, kind, id)
            .await
            .context("call delete")
    }

    pub async fn changes_since(
        &mut self,
        cursor: &str,
        limit: u32,
    ) -> Result<Result<types::ChangePage, types::BridgeError>> {
        self.bindings
            .hydra_bridge_adapter()
            .call_changes_since(&mut self.store, cursor, limit)
            .await
            .context("call changes-since")
    }

    pub fn fuel_remaining(&mut self) -> Result<u64> {
        self.store.get_fuel().context("read remaining fuel")
    }
}

impl host_if::Host for HostState {
    async fn http(
        &mut self,
        request: host_if::HttpRequest,
    ) -> Result<Result<host_if::HttpResponse, types::BridgeError>> {
        if !self.grant.origin_allowed(&request.url) {
            return Ok(Err(types::BridgeError::Invalid(format!(
                "origin not in grant: {}",
                redact_url(&request.url)
            ))));
        }

        match self
            .egress
            .send(
                &request.method,
                &request.url,
                &request.headers,
                request.body,
            )
            .await
        {
            Ok((status, headers, body)) => Ok(Ok(host_if::HttpResponse {
                status,
                headers,
                body,
            })),
            Err(error) => Ok(Err(types::BridgeError::Upstream(error.to_string()))),
        }
    }

    async fn secret(&mut self, name: String) -> Result<Result<String, types::BridgeError>> {
        if !self
            .grant
            .secret_names
            .iter()
            .any(|candidate| candidate == &name)
        {
            return Ok(Err(types::BridgeError::Invalid(format!(
                "secret not granted: {name}"
            ))));
        }

        match self.secrets.get(&name).await? {
            Some(value) => Ok(Ok(value)),
            None => Ok(Err(types::BridgeError::Invalid(format!(
                "secret missing in vault: {name}"
            )))),
        }
    }

    async fn kv_get(&mut self, key: String) -> Result<Option<String>> {
        self.kv.get(&key).await
    }

    async fn kv_set(&mut self, key: String, value: String) -> Result<()> {
        self.kv.set(&key, &value).await
    }

    async fn sql_query(
        &mut self,
        query: String,
        params: Vec<String>,
    ) -> Result<Result<String, types::BridgeError>> {
        let Some(sql) = self.sql.as_ref() else {
            return Ok(Err(types::BridgeError::Invalid("no DSN in grant".into())));
        };

        if !query
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("select")
        {
            return Ok(Err(types::BridgeError::Invalid(
                "read-only replica: SELECT only".into(),
            )));
        }

        match sql.query_json(&query, &params).await {
            Ok(payload) => Ok(Ok(payload)),
            Err(error) => Ok(Err(types::BridgeError::Upstream(error.to_string()))),
        }
    }

    async fn log(&mut self, level: String, message: String) -> Result<()> {
        let message: String = message
            .chars()
            .filter(|ch| !ch.is_control())
            .take(2048)
            .collect();
        match level.as_str() {
            "error" => tracing::error!(adapter = %self.grant.adapter_id, "{message}"),
            "warn" => tracing::warn!(adapter = %self.grant.adapter_id, "{message}"),
            _ => tracing::info!(adapter = %self.grant.adapter_id, "{message}"),
        }
        Ok(())
    }

    async fn now_ms(&mut self) -> Result<u64> {
        Ok(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64)
    }
}

fn redact_url(url: &str) -> String {
    url.split('?').next().unwrap_or(url).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use anyhow::Result;
    use store::TestDb;

    #[derive(Default)]
    struct MemoryKv {
        values: Mutex<HashMap<String, String>>,
    }

    #[async_trait]
    impl KvStore for MemoryKv {
        async fn get(&self, key: &str) -> Result<Option<String>> {
            Ok(self.values.lock().unwrap().get(key).cloned())
        }

        async fn set(&self, key: &str, value: &str) -> Result<()> {
            self.values
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
    }

    #[derive(Default)]
    struct CountingEgress {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl EgressClient for CountingEgress {
        async fn send(
            &self,
            _method: &str,
            _url: &str,
            _headers: &[(String, String)],
            _body: Option<Vec<u8>>,
        ) -> Result<(u16, Vec<(String, String)>, Vec<u8>)> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok((204, Vec::new(), Vec::new()))
        }
    }

    struct EchoSql;

    #[async_trait]
    impl ReplicaSql for EchoSql {
        async fn query_json(&self, query: &str, params: &[String]) -> Result<String> {
            Ok(format!("{query}::{params:?}"))
        }
    }

    fn grant(adapter_id: &str, fuel: u64) -> Grant {
        Grant {
            adapter_id: adapter_id.to_owned(),
            origins: vec!["https://allowed.example".into()],
            secret_names: vec!["api-token".into()],
            dsn_name: Some("replica".into()),
            fuel,
        }
    }

    fn state_with(
        grant: Grant,
        kv: Box<dyn KvStore>,
        secrets: Box<dyn SecretSource>,
        egress: Box<dyn EgressClient>,
        sql: Option<Box<dyn ReplicaSql>>,
    ) -> HostState {
        HostState {
            grant,
            kv,
            secrets,
            egress,
            sql,
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
        }
    }

    fn memcrm_component_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../adapters/memcrm.wasm")
            .canonicalize()
            .expect("memcrm adapter path")
    }

    #[tokio::test]
    async fn host_origin_denied_short_circuits_egress() -> Result<()> {
        let egress = Arc::new(CountingEgress::default());
        let mut state = state_with(
            grant("memcrm", 1024),
            Box::new(MemoryKv::default()),
            Box::new(StaticSecretSource::default()),
            Box::new(ArcEgress(egress.clone())),
            None,
        );

        let result = <HostState as host_if::Host>::http(
            &mut state,
            host_if::HttpRequest {
                method: "GET".into(),
                url: "https://blocked.example/api".into(),
                headers: Vec::new(),
                body: None,
            },
        )
        .await?;

        assert!(matches!(result, Err(types::BridgeError::Invalid(_))));
        assert_eq!(egress.calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[tokio::test]
    async fn host_secret_lookup_respects_grants() -> Result<()> {
        let mut state = state_with(
            grant("memcrm", 1024),
            Box::new(MemoryKv::default()),
            Box::new(StaticSecretSource::new(HashMap::from([(
                "api-token".into(),
                "secret-value".into(),
            )]))),
            Box::new(CountingEgress::default()),
            None,
        );

        let granted = <HostState as host_if::Host>::secret(&mut state, "api-token".into()).await?;
        match granted {
            Ok(value) => assert_eq!(value, "secret-value"),
            Err(error) => panic!("expected granted secret, got error: {error:?}"),
        }

        let denied = <HostState as host_if::Host>::secret(&mut state, "other".into()).await?;
        assert!(matches!(denied, Err(types::BridgeError::Invalid(_))));
        Ok(())
    }

    #[tokio::test]
    async fn host_sql_query_rejects_non_select() -> Result<()> {
        let mut state = state_with(
            grant("memcrm", 1024),
            Box::new(MemoryKv::default()),
            Box::new(StaticSecretSource::default()),
            Box::new(CountingEgress::default()),
            Some(Box::new(EchoSql)),
        );

        let result = <HostState as host_if::Host>::sql_query(
            &mut state,
            "INSERT INTO nope VALUES (1)".into(),
            Vec::new(),
        )
        .await?;

        assert!(matches!(result, Err(types::BridgeError::Invalid(_))));
        Ok(())
    }

    #[tokio::test]
    async fn host_store_kv_round_trip_is_adapter_scoped() -> Result<()> {
        let db = TestDb::new().await?;
        let result = async {
            let repo = AdapterKvRepo::new(db.pool.clone());
            let store_a = StoreKvStore::new(repo.clone(), "adapter-a");
            let store_b = StoreKvStore::new(repo, "adapter-b");

            store_a.set("cursor", "one").await?;
            store_b.set("cursor", "two").await?;

            assert_eq!(store_a.get("cursor").await?, Some("one".into()));
            assert_eq!(store_b.get("cursor").await?, Some("two".into()));
            Ok::<(), anyhow::Error>(())
        }
        .await;

        db.cleanup().await?;
        result
    }

    #[tokio::test]
    async fn host_instantiates_memcrm_component_async() -> Result<()> {
        let wasm = std::fs::read(memcrm_component_path())?;
        let host = BridgeHost::new()?;
        let mut handle = host
            .instantiate(
                &wasm,
                state_with(
                    grant("memcrm", 50_000),
                    Box::new(MemoryKv::default()),
                    Box::new(StaticSecretSource::default()),
                    Box::new(CountingEgress::default()),
                    None,
                ),
            )
            .await?;

        let descriptor = handle.describe().await?;
        assert_eq!(descriptor.name, "memcrm");
        assert!(descriptor.caps.read);
        Ok(())
    }

    #[tokio::test]
    async fn host_fuel_exhaustion_traps_guest() -> Result<()> {
        let wasm = std::fs::read(memcrm_component_path())?;
        let host = BridgeHost::new()?;
        let mut handle = host
            .instantiate(
                &wasm,
                state_with(
                    grant("memcrm", 0),
                    Box::new(MemoryKv::default()),
                    Box::new(StaticSecretSource::default()),
                    Box::new(CountingEgress::default()),
                    None,
                ),
            )
            .await?;

        let error = handle
            .describe()
            .await
            .expect_err("fuel exhaustion must trap");
        let chain = error
            .chain()
            .map(|cause| cause.to_string().to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            chain.contains("fuel") || chain.contains("trap"),
            "expected fuel/trap error chain, got: {chain}"
        );
        Ok(())
    }

    struct ArcEgress(Arc<CountingEgress>);

    #[async_trait]
    impl EgressClient for ArcEgress {
        async fn send(
            &self,
            method: &str,
            url: &str,
            headers: &[(String, String)],
            body: Option<Vec<u8>>,
        ) -> Result<(u16, Vec<(String, String)>, Vec<u8>)> {
            self.0.send(method, url, headers, body).await
        }
    }
}
