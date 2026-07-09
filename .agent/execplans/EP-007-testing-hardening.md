# EP-007 Testing Hardening

## 1. Purpose / Big Picture
Raise the floor: agent mesh v1 (DataSteward + BridgeEngineer synthesis loop with conformance gate + Comms draft), SuiteCRM-shape second fixture adapter for canary realism, failure-mode suites, regression net, flaky policy tooling, nightly soak.

## 2. Scope
crates/agents real behaviors (envelope-emitting only), fixtures/adapter-suitelike (deliberately quirky: dates as d/m/Y strings, paginated inconsistently, 429s, unicode names), canary shadow-sync diff in bridge-host, chaos/failure tests, CI nightly job.

## 3. Non-goals
No live SuiteCRM network dependency in CI (fixture only; live target is an operator activity in EP-010 D-notes); no social adapters; no email send (draft-only until EP-009 SMTP config).

## 4. Context and Orientation
BridgeEngineer here = the orchestration loop (discover→introspect→synthesize→conform→wire→canary) with the LLM synth step ROUTED through TK route `bridge_codegen` against the fake provider returning a stored known-good adapter source in tests — proving the loop, not the model. Real-model synthesis is runtime behavior gated at L3 (bridges.deploy_adapter).

## 5. Files to Read First
SPEC-009 (routes/contracts), reference/bridge/conformance.rs, crates/bridge-host loader, TESTING.md flaky policy.

## 6. Files to Change
crates/agents/src/{data_steward.rs,bridge_engineer.rs,comms.rs,prompts/*.md as include_str S1 segments}, fixtures/adapter-suitelike/*, crates/bridge-host/src/canary.rs, crates/*/tests/failure_*.rs & regress_*.rs, .github/workflows/nightly.yml, scripts/test-integration.sh (include failure suites), wiring/suitelike.map.yaml + transform lib crates/fabric/src/wiring.rs (fixed library per ADR-0007).

## 7. Interfaces and Contracts
`bridge_engineer::run(target) -> EnvelopeDraft(deploy_adapter{wasm_sha, conformance_report, wiring})`; canary: shadow-sync N=500 records, diff CDM projections old-vs-new adapter, report; wiring transforms exactly: trim,lower,upper,titlecase,phone_e164,usd_to_cents,date_iso(fmt),lookup(table),split(sep,idx),concat(sep),const(v).

## 8. Milestones
M1 Wiring engine + suitelike fixture + mapping conf review-queue (<0.90 queued). Validation: `cargo test -p fabric wiring_` → `m1: ok`. Recovery: date parsing quirks are the point — fixtures define truth.
M2 Conformance vs suitelike (finds seeded quirks; harden harness until green-with-quirks-documented). Validation: `cargo test -p bridge-host --test conformance -- suitelike` → `m2: ok`.
M3 BridgeEngineer loop end-to-end in tests (fake LLM returns adapter source → build → conform → wire → canary → envelope). Validation: `cargo test -p agents --test bridge_engineer_loop` → `m3: ok`. Recovery: repair-round path tested by fake returning broken source first round.
M4 DataSteward (dedupe proposals via identity + envelope merge) & Comms (draft_email contract via TK). Validation: `cargo test -p agents` → `m4: ok` incl. TK-1 grep invariant test.
M5 Failure-mode + regression + nightly. Edits: failure suites (PG down mid-tx, NATS outage buffering, adapter fuel-trap, provider 500 fallback, nuke repair-once), nightly.yml runs soak+conformance-ignored+cache-audit. Validation: `bash scripts/verify.sh` 3× consecutive green (record timestamps in Outcomes) → treat as `m5: ok`. Recovery: flaky found ⇒ apply TESTING flaky policy same day.

## 9. Concrete Steps
Order above.

## 10. Validation and Acceptance
verify ×3 green; nightly file present; regression names `regress_*` ≥ 5; diff ⊆ §6.

## 11. Idempotence and Recovery
Fixtures deterministic (seeded); canary diff pure.

## 12. Progress
- [x] M1 - [x] M2 - [x] M3 - [x] M4 - [x] M5

## 13. Surprises & Discoveries
## 14. Decision Log
## 15. Outcomes & Retrospective
