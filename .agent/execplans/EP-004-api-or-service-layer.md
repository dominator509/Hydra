# EP-004 Service Layer: fabric REST+MCP, LLM Router, TOKENKILLER, Bridge Host + Conformance

## 1. Purpose / Big Picture
The system grows its nervous system: HTTP/MCP surface (SPEC-003), multi-provider LLM router with structural PII gate, the full TOKENKILLER pipeline (SPEC-009), the Wasmtime bridge host with capability grants, and the conformance harness — plus the first hand-written fixture adapter proving the WIT contract before any agent ever generates one.

## 2. Scope
crates/{fabric,llm-router,tokenkiller,bridge-host,bridge-wit}, wit/hydra-bridge.wit, one fixture adapter (Rust→wasm, in-memory CRM), tests/fixtures/tk-corpus, scripts/cache-hit-audit.sh, kernel executor path (Decision::Execute → apply+events).

## 3. Non-goals
No shell views (EP-005); no OAuth provider (EP-006 — Bearer stub validates a static dev token behind HYDRA_ENV=dev); no agent behaviors beyond a smoke `concierge.ping` route proving the TK call path; no GraphQL; no real SuiteCRM adapter (EP-007 canary uses it, synthesis is agent-runtime feature not build-time).

## 4. Context and Orientation
This is the largest plan; milestones are strictly ordered so each lands green. Copy-adapt: reference/bridge/hydra-bridge.wit, reference/bridge/host.rs, reference/bridge/conformance.rs, reference/tokenkiller/{canon,prefix,nukeguard,ledger,contracts}.rs, reference/router.rs. SPEC-003/006/009 normative.

## 5. Files to Read First
SPEC-003, SPEC-009, SPEC-006, all reference/ files above, ARCHITECTURE (egress + TK boundaries), crates/store public API.

## 6. Files to Change
wit/hydra-bridge.wit; crates/bridge-wit/{Cargo.toml,src/lib.rs,build via wit-bindgen}; crates/bridge-host/src/{lib.rs,host.rs,grants.rs,loader.rs}; crates/bridge-host/tests/conformance.rs; fixtures/adapter-memcrm/ (Rust cdylib→component) + adapters/memcrm.wasm build script scripts/build-adapters.sh; crates/tokenkiller/src/{lib.rs,canon.rs,prefix.rs,nukeguard.rs,contracts.rs,ledger.rs,session.rs}+tests; crates/llm-router/src/{lib.rs,providers/{anthropic.rs,deepseek.rs,openai_compat.rs},routes.rs}+tests(wiremock incl. deepseek cache fake); crates/fabric/src/{lib.rs,rest/*.rs,mcp.rs,egress.rs,error.rs,services.rs}; crates/kernel/src/executor.rs; tests/fixtures/tk-corpus/*.json; scripts/cache-hit-audit.sh; docker/compose.yaml (+egress-proxy placeholder note only).

## 7. Interfaces and Contracts
Exact WIT world `hydra:bridge@1.0.0` as reference file (adapter exports describe/probe/introspect-schema/list/get/upsert/delete/changes-since; host exports http/secret/kv/sql/log/now-ms). Router: `trait LlmProvider { async fn complete(&self, ChatRequest) -> Result<ProviderResponse>; fn tags(&self)->&[Tag]; }` ProviderResponse carries `CacheUsage{hit_tokens,miss_tokens}` (zeros if unsupported). TK: `Session::complete(route:&str, s: Segments, tail: Tail) -> Result<Contracted, TkError>` — the ONLY function agents may call for LLM work (TK-1). REST routes exactly SPEC-003 list.

## 8. Milestones
M1 WIT + bindings + memcrm adapter builds. Edits: copy wit; bridge-wit generates guest+host bindings; fixtures/adapter-memcrm implements adapter over an in-RAM map with etags+cursor pagination+synthetic 429 every Nth call (cfg via probe config-json); scripts/build-adapters.sh (`cargo build --target wasm32-wasip2` + wasm-tools component check) → `adapters: ok`. Validation: `bash scripts/build-adapters.sh && wasm-tools validate adapters/memcrm.wasm && echo m1: ok`. Expected: `m1: ok`. Recovery: wit-bindgen version mismatch → pin per Cargo.lock; component-model errors name the missing export.
M2 Bridge host + grants + fuel. Edits: reference/host.rs adapted: Wasmtime engine (component model, async), GrantTable{origins,secret_names,dsn_name,fuel}, host.http → egress stub enforcing allow-list (real proxy client target; local = direct reqwest behind trait), kv→store.adapter_kv, secret→vault stub (dev map), fuel set+trap on exhaust. Validation: `cargo test -p bridge-host host_` → `m2: ok`. Recovery: async component instantiation requires `Config::async_support(true)` — check first.
M3 Conformance harness green on memcrm. Edits: reference/conformance.rs adapted; property suite (proptest strategies for records/kinds), checks per SPEC-003 bridge section + TESTING conformance list; 10k soak `#[ignore]`. Validation: `cargo test -p bridge-host --test conformance` → `m3: ok`. Expected: `m3: ok`. Recovery: failures print the generated case; add as fixed regress fixture.
M4 TOKENKILLER core. Edits: canon (sorted-key writer over serde_json::Value, NFC via unicode-normalization, ryu floats), prefix (segments S0..S3, tokenizer adapter trait + `ApproxTokenizer` for tests, 64-block pad, debug_assert_stable), nukeguard (state machine over byte stream w/ trip table from SPEC-009 TK5), contracts (EnvelopeProposal/UnifiedDiff/MappingYaml validators), ledger writer→store, session (assemble→router→guard→contract→ledger, repair-once). Validation: `cargo test -p tokenkiller` (incl. proptest canon idempotence, alignment test, trip table) → `m4: ok`. Recovery: NFC or float formatting diffs show as sha mismatch in debug_assert_stable — print both byte streams in the failing test.
M5 Router + providers + PII gate + deepseek cache fake. Edits: three providers (reqwest via egress client), routes.yaml loader, structural PII gate (INV-4), fallback chain, cost estimate; wiremock deepseek fake computes hit/miss by longest-common-prefix over stored prior prompts at 64-token granularity (ApproxTokenizer) and returns usage fields. Validation: `cargo test -p llm-router` → `m5: ok` (must include `pii_gate_blocks`, `fallback_chain`, `deepseek_usage_fields`). Recovery: wiremock matcher on body prefix → match on route header instead; fake stores per-test state in Arc<Mutex>.
M6 Corpus + cache-hit-audit ≥0.97. Edits: tk-corpus: 3 routes × ~14 sequential calls each mirroring agent loops (stable S0–S2 fixtures, growing frozen tails); scripts/cache-hit-audit.sh runs `cargo test -p tokenkiller --test replay_corpus -- --nocapture` which drives session against the fake and prints ratio; script greps ratio ≥ ${TK_HIT_RATIO_TARGET:-0.97}. Validation: `bash scripts/cache-hit-audit.sh` → `cache-hit audit: ok (ratio=0.9XX)`. Expected: ratio ≥0.97 printed. Recovery: ratio low ⇒ print per-call prefix_sha transitions; the first call whose sha differs from predecessor identifies the unstable segment; fix canon/pad, never fudge corpus.
M7 fabric REST+MCP+executor+ping-agent. Edits: SPEC-003 routes over service traits; problem+json per SPEC-006; MCP server (stdio+HTTP) exposing the 7 tools; kernel executor consumes Decision::Execute tokens, applies via store, emits events, records receipts; `concierge.ping` internal route: builds a real TK session (route "concierge") answering a canned question via fake provider in tests. Validation: `bash scripts/test-integration.sh` → `integration tests: ok` (incl. contract parity test, envelope approve flow, mcp schema snapshot). Recovery: utoipa mismatch lists the route — fix annotation not the test.

## 9. Concrete Steps
Strict milestone order; commit per milestone; run `bash scripts/format-check.sh && bash scripts/lint.sh` before each commit.

## 10. Validation and Acceptance
verify.sh green; cache-hit-audit ok ≥0.97; conformance green; `rg 'llm_router' crates/agents crates/fabric` shows imports ONLY in tokenkiller+router themselves (TK-1 grep check); OpenAPI served; diff ⊆ §6.

## 11. Idempotence and Recovery
All builds/tests re-runnable; adapters rebuild deterministic; resume = verify.sh first red, then that milestone's validation.

## 12. Progress
- [ ] M1 - [ ] M2 - [ ] M3 - [ ] M4 - [ ] M5 - [ ] M6 - [ ] M7

## 13. Surprises & Discoveries
## 14. Decision Log
## 15. Outcomes & Retrospective
