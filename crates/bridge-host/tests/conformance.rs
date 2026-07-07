use std::collections::{BTreeSet, HashMap};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, OnceLock,
};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use bridge_host::bindings::hydra::bridge::types::{self, BridgeError, ChangeOp, RawRecord};
use bridge_host::{
    default_adapter_path, load_component_bytes, AdapterHandle, BridgeHost, EgressClient, Grant,
    HostState, KvStore, SecretSource, StaticSecretSource,
};
use proptest::prelude::*;
use serde_json::{json, Value};
use wasmtime::component::ResourceTable;
use wasmtime_wasi::WasiCtxBuilder;

const ADAPTER: &str = "memcrm";
const KIND: &str = "party";
const LIST_LIMIT: u32 = 17;
const DEFAULT_FUEL: u64 = 20_000_000;

static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Default)]
struct MemoryKv {
    values: Mutex<HashMap<String, String>>,
}

#[async_trait]
impl KvStore for MemoryKv {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .values
            .lock()
            .map_err(|_| anyhow!("memory kv poisoned"))?
            .get(key)
            .cloned())
    }

    async fn set(&self, key: &str, value: &str) -> Result<()> {
        self.values
            .lock()
            .map_err(|_| anyhow!("memory kv poisoned"))?
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

fn grant(origins: Vec<String>, fuel: u64) -> Grant {
    Grant {
        adapter_id: ADAPTER.to_owned(),
        origins,
        secret_names: Vec::new(),
        dsn_name: None,
        fuel,
    }
}

fn host_state(grant: Grant, egress: Box<dyn EgressClient>) -> HostState {
    HostState {
        grant,
        kv: Box::new(MemoryKv::default()),
        secrets: Box::new(StaticSecretSource::default()) as Box<dyn SecretSource>,
        egress,
        sql: None,
        wasi: WasiCtxBuilder::new().build(),
        table: ResourceTable::new(),
    }
}

struct Ctx {
    handle: AdapterHandle,
    egress: Arc<CountingEgress>,
}

impl Ctx {
    async fn new(config: Value) -> Result<Self> {
        Self::new_with_grant(
            config,
            grant(vec!["https://allowed.example".into()], DEFAULT_FUEL),
        )
        .await
    }

    async fn new_with_grant(config: Value, grant: Grant) -> Result<Self> {
        let egress = Arc::new(CountingEgress::default());
        let mut handle = bridge_host()?
            .instantiate(
                adapter_wasm()?,
                host_state(grant, Box::new(ArcEgress(egress.clone()))),
            )
            .await?;

        let config_json = serde_json::to_string(&config).context("serialize probe config")?;
        let descriptor = bridge_ok(handle.probe(&config_json).await?)?;
        if descriptor.name != ADAPTER {
            bail!("unexpected descriptor name: {}", descriptor.name);
        }

        Ok(Self { handle, egress })
    }

    async fn list_raw(
        &mut self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<std::result::Result<types::Page, BridgeError>> {
        self.handle.list(KIND, cursor, limit).await
    }

    async fn list_page(&mut self, cursor: Option<&str>, limit: u32) -> Result<types::Page> {
        bridge_ok(self.list_raw(cursor, limit).await?)
    }

    async fn list_all(&mut self, limit: u32) -> Result<Vec<RawRecord>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = self.list_page(cursor.as_deref(), limit).await?;
            cursor = page.next_cursor.clone();
            out.extend(page.records);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    async fn get_raw(&mut self, id: &str) -> Result<std::result::Result<RawRecord, BridgeError>> {
        self.handle.get(KIND, id).await
    }

    async fn get_existing(&mut self, id: &str) -> Result<RawRecord> {
        bridge_ok(self.get_raw(id).await?)
    }

    async fn upsert_raw(
        &mut self,
        record: RawRecord,
    ) -> Result<std::result::Result<RawRecord, BridgeError>> {
        self.handle.upsert(record).await
    }

    async fn upsert_value(
        &mut self,
        id: &str,
        data: Value,
        etag: Option<String>,
    ) -> Result<RawRecord> {
        let record = make_record(id, data, etag)?;
        bridge_ok(self.upsert_raw(record).await?)
    }

    async fn delete_raw(&mut self, id: &str) -> Result<std::result::Result<(), BridgeError>> {
        self.handle.delete(KIND, id).await
    }

    async fn delete_ok(&mut self, id: &str) -> Result<()> {
        bridge_ok(self.delete_raw(id).await?)
    }

    async fn changes_since_raw(
        &mut self,
        cursor: &str,
        limit: u32,
    ) -> Result<std::result::Result<types::ChangePage, BridgeError>> {
        self.handle.changes_since(cursor, limit).await
    }

    async fn changes_since(&mut self, cursor: &str, limit: u32) -> Result<types::ChangePage> {
        bridge_ok(self.changes_since_raw(cursor, limit).await?)
    }

    fn egress_calls(&self) -> usize {
        self.egress.calls.load(Ordering::SeqCst)
    }

    fn fuel_remaining(&mut self) -> Result<u64> {
        self.handle.fuel_remaining()
    }
}

fn bridge_host() -> Result<&'static BridgeHost> {
    static HOST: OnceLock<BridgeHost> = OnceLock::new();
    if let Some(host) = HOST.get() {
        return Ok(host);
    }

    let host = BridgeHost::new()?;
    let _ = HOST.set(host);
    HOST.get()
        .ok_or_else(|| anyhow!("bridge host cache failed to initialize"))
}

fn adapter_wasm() -> Result<&'static [u8]> {
    static ADAPTER_WASM: OnceLock<Vec<u8>> = OnceLock::new();
    if let Some(wasm) = ADAPTER_WASM.get() {
        return Ok(wasm.as_slice());
    }

    let wasm = load_component_bytes(default_adapter_path(ADAPTER))
        .with_context(|| format!("load {} adapter bytes", ADAPTER))?;
    let _ = ADAPTER_WASM.set(wasm);
    let wasm = ADAPTER_WASM
        .get()
        .ok_or_else(|| anyhow!("adapter wasm cache failed to initialize"))?;
    Ok(wasm.as_slice())
}

fn bridge_ok<T>(result: std::result::Result<T, BridgeError>) -> Result<T> {
    result.map_err(bridge_error)
}

fn bridge_error(error: BridgeError) -> anyhow::Error {
    match error {
        BridgeError::AuthExpired(message) => anyhow!("bridge auth-expired: {message}"),
        BridgeError::RateLimited(retry_after) => anyhow!("bridge rate-limited: {retry_after}"),
        BridgeError::Conflict(message) => anyhow!("bridge conflict: {message}"),
        BridgeError::NotFound(message) => anyhow!("bridge not-found: {message}"),
        BridgeError::Invalid(message) => anyhow!("bridge invalid: {message}"),
        BridgeError::Upstream(message) => anyhow!("bridge upstream: {message}"),
    }
}

fn parse_record_data(record: &RawRecord) -> Result<Value> {
    serde_json::from_str(&record.data)
        .with_context(|| format!("parse record data for {}", record.id))
}

fn make_record(id: &str, data: Value, etag: Option<String>) -> Result<RawRecord> {
    Ok(RawRecord {
        kind: KIND.to_owned(),
        id: id.to_owned(),
        etag,
        data: serde_json::to_string(&data).context("serialize raw record data")?,
    })
}

fn next_id(prefix: &str) -> String {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    format!("{prefix}-{id:06}")
}

fn seed_record(index: usize) -> Value {
    let id = format!("party-{index:04}");
    json!({
        "id": id,
        "data": {
            "name": format!("Party {index:04}"),
            "email": format!("party{index:04}@example.test"),
            "phone": format!("+1425{index:07}"),
            "status": if index.is_multiple_of(2) { "lead" } else { "active" },
        }
    })
}

fn seed_config(count: usize) -> Value {
    let seed = (0..count).map(seed_record).collect::<Vec<_>>();
    json!({
        "seed": {
            KIND: seed,
        }
    })
}

fn arb_party() -> impl Strategy<Value = Value> {
    let names = vec![
        "Alice Example".to_owned(),
        "Zoë de León".to_owned(),
        "李 小龙".to_owned(),
        "Renée O'Neil".to_owned(),
        "أمل سالم".to_owned(),
        "Ирина Смирнова".to_owned(),
    ];
    let users = vec![
        "alpha".to_owned(),
        "beta".to_owned(),
        "gamma".to_owned(),
        "zoe".to_owned(),
        "amal".to_owned(),
        "irina".to_owned(),
    ];
    let statuses = vec!["lead".to_owned(), "active".to_owned(), "paused".to_owned()];

    (
        prop::sample::select(names),
        prop::sample::select(users),
        prop::sample::select(statuses),
        0u64..10_000_000u64,
    )
        .prop_map(|(name, user, status, phone)| {
            json!({
                "name": name,
                "email": format!("{user}{phone}@example.test"),
                "phone": format!("+1425{:07}", phone % 10_000_000),
                "status": status,
            })
        })
}

fn prop_error(message: impl Into<String>) -> TestCaseError {
    TestCaseError::fail(message.into())
}

fn tokio_prop<F>(future: F) -> Result<(), TestCaseError>
where
    F: std::future::Future<Output = Result<(), TestCaseError>>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| prop_error(error.to_string()))?
        .block_on(future)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..Default::default() })]
    #[test]
    fn c1_crud_roundtrip(rec in arb_party()) {
        tokio_prop(async move {
            let mut cx = Ctx::new(json!({})).await.map_err(|error| prop_error(error.to_string()))?;
            let id = next_id("crud");
            let saved = cx
                .upsert_value(&id, rec.clone(), None)
                .await
                .map_err(|error| prop_error(error.to_string()))?;
            let got = cx
                .get_existing(&saved.id)
                .await
                .map_err(|error| prop_error(error.to_string()))?;
            let got_data =
                parse_record_data(&got).map_err(|error| prop_error(error.to_string()))?;

            for key in ["name", "email", "phone", "status"] {
                prop_assert_eq!(
                    &got_data[key],
                    &rec[key],
                    "field {} mutated in round-trip",
                    key
                );
            }
            Ok(())
        })?;
    }
}

#[tokio::test]
async fn c2_pagination_exhaustive_and_cursor_stable() -> Result<()> {
    let mut cx = Ctx::new(seed_config(250)).await?;
    let expected_ids = (0..250)
        .map(|index| format!("party-{index:04}"))
        .collect::<Vec<_>>();
    let expected_set = expected_ids.iter().cloned().collect::<BTreeSet<_>>();

    let first_page = cx.list_page(None, LIST_LIMIT).await?;
    let saved_cursor = first_page
        .next_cursor
        .clone()
        .ok_or_else(|| anyhow!("expected pagination cursor after first page"))?;
    let first_page_ids = first_page
        .records
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();

    let mut seen = first_page.records;
    let mut cursor = Some(saved_cursor.clone());
    while let Some(current) = cursor {
        let page = cx.list_page(Some(current.as_str()), LIST_LIMIT).await?;
        cursor = page.next_cursor.clone();
        seen.extend(page.records);
    }

    let seen_ids = seen
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        seen_ids, expected_set,
        "expected to walk every seeded id exactly once"
    );
    assert_eq!(
        seen.len(),
        expected_ids.len(),
        "pagination duplicated one or more records"
    );

    let replay_page = cx
        .list_page(Some(saved_cursor.as_str()), LIST_LIMIT)
        .await?;
    let replay_ids = replay_page
        .records
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    assert!(
        replay_ids.is_disjoint(&first_page_ids),
        "replaying from saved cursor repeated records from earlier pages"
    );

    Ok(())
}

#[tokio::test]
async fn c3_upsert_idempotent() -> Result<()> {
    let mut cx = Ctx::new(json!({})).await?;
    let id = next_id("idempotent");
    let data = json!({
        "name": "Idempotent Party",
        "email": "idempotent@example.test",
        "phone": "+14255550101",
        "status": "active",
    });

    let first = cx.upsert_value(&id, data.clone(), None).await?;
    let second = cx
        .upsert_value(&id, data.clone(), first.etag.clone())
        .await?;
    let listed = cx.list_all(50).await?;
    let changes = cx.changes_since("", 50).await?;

    assert_eq!(parse_record_data(&first)?, parse_record_data(&second)?);
    assert_eq!(
        first.etag, second.etag,
        "identical upsert should not mint a new etag"
    );
    assert_eq!(
        listed.iter().filter(|record| record.id == id).count(),
        1,
        "identical upsert should not duplicate records"
    );
    assert_eq!(
        changes
            .changes
            .iter()
            .filter(|change| change.rec.id == id && change.op == ChangeOp::Upserted)
            .count(),
        1,
        "identical upsert should emit at most one upsert change"
    );

    Ok(())
}

#[tokio::test]
async fn c4_etag_conflict() -> Result<()> {
    let mut cx = Ctx::new(json!({})).await?;
    let id = next_id("etag");
    let original = json!({
        "name": "Before",
        "email": "before@example.test",
        "phone": "+14255550110",
        "status": "lead",
    });
    let changed = json!({
        "name": "After",
        "email": "after@example.test",
        "phone": "+14255550111",
        "status": "active",
    });

    let first = cx.upsert_value(&id, original.clone(), None).await?;
    let updated = cx
        .upsert_value(&id, changed.clone(), first.etag.clone())
        .await?;
    let stale = cx
        .upsert_raw(make_record(&id, original, first.etag.clone())?)
        .await?;

    match stale {
        Err(BridgeError::Conflict(_)) => {}
        Ok(_) => bail!("expected stale etag conflict, but upsert succeeded"),
        Err(other) => bail!("expected stale etag conflict, got {other:?}"),
    }

    let got = cx.get_existing(&id).await?;
    assert_eq!(
        parse_record_data(&got)?,
        changed,
        "record changed after stale conflict"
    );
    assert_eq!(
        got.etag, updated.etag,
        "stale conflict should not advance etag"
    );
    Ok(())
}

#[tokio::test]
async fn c5_rate_limit_surfaces() -> Result<()> {
    let mut cx = Ctx::new(json!({ "rate_limit_every": 1 })).await?;
    let started = Instant::now();
    let result = cx.list_raw(None, LIST_LIMIT).await?;
    let elapsed = started.elapsed();

    match result {
        Err(BridgeError::RateLimited(retry_after)) => {
            assert_eq!(
                retry_after, 2,
                "fixture should surface retry-after verbatim"
            );
        }
        Ok(_) => bail!("expected rate-limit error, list succeeded"),
        Err(other) => bail!("expected rate-limit error, got {other:?}"),
    }
    assert!(
        elapsed < Duration::from_millis(100),
        "adapter hid retry handling behind a sleep: {elapsed:?}"
    );
    Ok(())
}

#[tokio::test]
async fn c6_changes_since_monotonic() -> Result<()> {
    let mut cx = Ctx::new(json!({})).await?;
    let first_id = next_id("changes");
    let second_id = next_id("changes");

    let baseline = cx.changes_since("", 10).await?;
    assert!(
        baseline.changes.is_empty(),
        "fresh adapter should have no initial change feed"
    );
    assert_eq!(
        baseline.next_cursor, "",
        "fresh adapter should start at the empty cursor"
    );

    cx.upsert_value(
        &first_id,
        json!({
            "name": "First",
            "email": "first@example.test",
            "phone": "+14255550201",
            "status": "lead",
        }),
        None,
    )
    .await?;
    cx.upsert_value(
        &second_id,
        json!({
            "name": "Second",
            "email": "second@example.test",
            "phone": "+14255550202",
            "status": "active",
        }),
        None,
    )
    .await?;
    cx.delete_ok(&first_id).await?;

    let page = cx.changes_since(&baseline.next_cursor, 10).await?;
    let seen = page
        .changes
        .iter()
        .map(|change| (change.rec.id.clone(), change.op))
        .collect::<Vec<_>>();
    assert_eq!(
        seen,
        vec![
            (first_id.clone(), ChangeOp::Upserted),
            (second_id.clone(), ChangeOp::Upserted),
            (first_id.clone(), ChangeOp::Deleted),
        ],
        "changes-since should return mutations in order"
    );

    let stable_cursor = page.next_cursor.clone();
    let repeated = cx.changes_since(&stable_cursor, 10).await?;
    assert!(
        repeated.changes.is_empty(),
        "stable cursor replayed earlier changes"
    );
    assert_eq!(
        repeated.next_cursor, stable_cursor,
        "stable cursor should remain unchanged when there are no new mutations"
    );

    cx.upsert_value(
        &first_id,
        json!({
            "name": "First Reloaded",
            "email": "first.reloaded@example.test",
            "phone": "+14255550203",
            "status": "active",
        }),
        None,
    )
    .await?;

    let delta = cx.changes_since(&stable_cursor, 10).await?;
    assert_eq!(
        delta.changes.len(),
        1,
        "new cursor should see only new mutations"
    );
    assert_eq!(delta.changes[0].rec.id, first_id);
    assert_eq!(delta.changes[0].op, ChangeOp::Upserted);

    let stable_value = stable_cursor
        .parse::<u64>()
        .context("parse stable cursor as u64")?;
    let delta_value = delta
        .next_cursor
        .parse::<u64>()
        .context("parse delta cursor as u64")?;
    assert!(
        delta_value > stable_value,
        "returned cursor should move forward after new mutations"
    );

    Ok(())
}

#[tokio::test]
async fn c7_unicode_and_edges() -> Result<()> {
    let mut cx = Ctx::new(json!({})).await?;
    let id = next_id("unicode");
    let note = "🙂".repeat(2_500);
    let payload = json!({
        "name": "Cafe\u{301} / Café / مرحبا / שלום",
        "email": "unicode@example.test",
        "phone": "+14255550300",
        "status": "paused",
        "note": note,
        "rtl": "مرحبا بالعالم",
        "emoji": "🛰️🚀",
    });

    cx.upsert_value(&id, payload.clone(), None).await?;
    let got = cx.get_existing(&id).await?;
    assert_eq!(
        parse_record_data(&got)?,
        payload,
        "unicode or large payload mutated in round-trip"
    );
    Ok(())
}

#[tokio::test]
async fn c8_grant_denial_graceful() -> Result<()> {
    let mut cx = Ctx::new_with_grant(json!({}), grant(Vec::new(), DEFAULT_FUEL)).await?;
    let id = next_id("grant");
    let payload = json!({
        "name": "No Egress Required",
        "email": "grant@example.test",
        "phone": "+14255550400",
        "status": "active",
    });

    let saved = cx.upsert_value(&id, payload.clone(), None).await?;
    let got = cx.get_existing(&id).await?;

    assert_eq!(parse_record_data(&saved)?, payload);
    assert_eq!(parse_record_data(&got)?, payload);
    assert_eq!(
        cx.egress_calls(),
        0,
        "memcrm should not attempt host egress during core CRUD operations"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "nightly soak"]
async fn c9_soak_10k() -> Result<()> {
    let mut cx = Ctx::new_with_grant(
        seed_config(10_000),
        grant(vec!["https://allowed.example".into()], 20_000_000),
    )
    .await?;
    let records = cx.list_all(200).await?;
    assert_eq!(
        records.len(),
        10_000,
        "soak seed should enumerate every record"
    );
    assert!(
        cx.fuel_remaining()? > 0,
        "soak should leave some fuel budget"
    );
    Ok(())
}
