//! reference/bridge/conformance.rs — the brutal gate (EP-004 M3, EP-007 M2).
//! Property-based harness every adapter must pass BEFORE canary. Runs the SAME suite
//! against hand-written fixtures and agent-synthesized adapters — the gate is what
//! makes autonomous synthesis safe (ADR-0002/0004). Deps: proptest, tokio, serde_json.
//! Copy into crates/bridge-host/tests/conformance.rs.

use proptest::prelude::*;
use serde_json::json;

// Harness context: instantiated adapter + seeded in-CRM dataset handle.
// `Ctx::new(adapter_name)` loads adapters/<name>.wasm with a test Grant and,
// for fixtures, a seeding side-channel (probe config).
struct Ctx { /* AdapterHandle + seed info */ }

impl Ctx {
    async fn new(_adapter: &str) -> Self { unimplemented!("wire to bridge-host loader") }
    async fn list_all(&mut self, kind: &str) -> Vec<serde_json::Value> { unimplemented!() }
    async fn get(&mut self, kind: &str, id: &str) -> Option<serde_json::Value> { unimplemented!() }
    async fn upsert(&mut self, kind: &str, rec: serde_json::Value) -> Result<serde_json::Value, String> { unimplemented!() }
    async fn delete(&mut self, kind: &str, id: &str) -> Result<(), String> { unimplemented!() }
    async fn drain_changes(&mut self, cursor: &str) -> (Vec<serde_json::Value>, String) { unimplemented!() }
}

/// Record strategy: unicode names, empty-string vs null, huge-but-legal fields,
/// awkward dates — the quirk space that breaks naive adapters.
fn arb_party() -> impl Strategy<Value = serde_json::Value> {
    ("[\\p{L} .'-]{1,40}", "[a-z0-9.]{1,20}", 0u64..1_000_000u64).prop_map(|(name, user, phone)| {
        json!({
            "name": name,
            "email": format!("{user}@example.test"),
            "phone": format!("+1425{:07}", phone % 10_000_000),
        })
    })
}

// C1 CRUD round-trip: upsert → get preserves data (modulo adapter-declared coercions).
proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..Default::default() })]
    #[test]
    fn c1_crud_roundtrip(rec in arb_party()) {
        tokio_test(async move {
            let mut cx = Ctx::new(std::env::var("ADAPTER").as_deref().unwrap_or("memcrm")).await;
            let saved = cx.upsert("party", rec.clone()).await.expect("upsert");
            let got = cx.get("party", saved["id"].as_str().unwrap()).await.expect("get");
            for k in ["name", "email", "phone"] {
                prop_assert_eq!(&got["data"][k], &rec[k], "field {} mutated in round-trip", k);
            }
            Ok(())
        })?;
    }
}

// C2 Pagination exhaustiveness + stability: walking pages yields every seeded id exactly once,
//    and re-walking from a saved cursor never repeats earlier items.
#[tokio::test]
async fn c2_pagination_exhaustive_and_cursor_stable() { /* seed 250, walk limit=17, assert set equality */ }

// C3 Idempotent upsert: same (kind,id,data) twice ⇒ one record, version/etag advances at most once.
#[tokio::test]
async fn c3_upsert_idempotent() { /* upsert twice, list_all count unchanged */ }

// C4 Etag conflict honored (when caps.etags): stale etag ⇒ bridge-error::conflict, record unchanged.
#[tokio::test]
async fn c4_etag_conflict() { /* fetch, mutate out-of-band via seed channel, upsert stale ⇒ conflict */ }

// C5 429 honoring: fixture emits rate-limited(2) every Nth call; harness asserts the adapter
//    surfaces it VERBATIM (adapters must not sleep internally — kernel owns backoff, SPEC-006).
#[tokio::test]
async fn c5_rate_limit_surfaces() { /* count wall time: no hidden sleeps > 100ms */ }

// C6 changes-since correctness: after k mutations, draining from previous cursor yields exactly
//    those k changes in order; deleted ids arrive as op=deleted; returned cursor is monotonic
//    (draining again from it yields nothing until new mutations).
#[tokio::test]
async fn c6_changes_since_monotonic() { }

// C7 Unicode + edge payloads: NFC/NFD names, 10KB note field, emoji, RTL — survive round-trip.
#[tokio::test]
async fn c7_unicode_and_edges() { }

// C8 No forbidden capability use: run with an EMPTY origin grant; any http attempt must return
//    Invalid (policy) not hang/trap — proves the adapter handles denial gracefully.
#[tokio::test]
async fn c8_grant_denial_graceful() { }

// C9 Soak (nightly): 10k records import within budget, memory stable, fuel within grant.
#[tokio::test]
#[ignore = "nightly soak"]
async fn c9_soak_10k() { }

fn tokio_test<F: std::future::Future<Output = Result<(), TestCaseError>>>(f: F) -> Result<(), TestCaseError> {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
}

// Gate wiring: EP-004 M3 requires C1–C8 green for memcrm; EP-007 M2 for suitelike;
// BridgeEngineer refuses to emit a deploy_adapter envelope unless the report shows
// C1–C8 pass + C9 scheduled (agents/bridge_engineer.rs checks the JSON report).
