# EP-002 Core Domain (CDM + Governor)

## 1. Purpose / Big Picture
Implement SPEC-001: pure L1 domain — kind registry with JSON Schema validation, entity/edge types, identity resolution proposals, the ActionEnvelope state machine, and the deterministic Governor. This is the safety heart; everything later trusts it.

## 2. Scope
crates/cdm and crates/governor only, plus their unit/property tests.

## 3. Non-goals
No SQL, no HTTP, no async runtime in these crates (governor is sync), no LLM, no config file loading (structs accept parsed config; parsing lives in kernel later).

## 4. Context and Orientation
SPEC-001 is normative. reference/governor.rs is the copy-adapt base for the envelope machine + evaluate(); extend it to full SPEC coverage (matrix resolution specificity, sealed Execute token).

## 5. Files to Read First
.agent/specs/SPEC-001-core-domain.md, reference/governor.rs, ARCHITECTURE.md layer rules, crates/cdm/src/lib.rs, crates/governor/src/lib.rs.

## 6. Files to Change
crates/cdm/src/{lib.rs,kinds.rs,schema.rs,identity.rs}, crates/cdm/tests/*, crates/governor/src/{lib.rs,envelope.rs,policy.rs,constitution.rs,decision.rs}, crates/governor/tests/*, Cargo.toml files (deps: serde, serde_json, jsonschema, uuid, thiserror, proptest[dev], time).

## 7. Interfaces and Contracts
Public API (exact names — later crates compile against these):
cdm: `KindRegistry::register(kind:&str, schema: serde_json::Value) -> Result<(),DomainError>`, `validate(kind,&Value)`, `Entity`, `Edge`, `identity::proposals(&[PartyView]) -> Vec<MergeProposal>`.
governor: `Level`, `ActionEnvelope`, `EnvelopeState`, `Reversal`, `BlastRadius`, `PolicyMatrix::resolve(domain,action,kind)->Cell`, `Constitution::check(&ActionEnvelope, &SpendSnapshot)->Result<(),Rule>`, `Governor::evaluate(&ActionEnvelope,&SpendSnapshot)->Decision`, `Decision::{Block(String),SuggestOnly,Queue,Execute(ExecuteToken)}` where `ExecuteToken` has private constructor (sealed) — kernel executor requires it.

## 8. Milestones
M1 cdm kinds+schema. Edits: builtin kind schemas (party, deal, pipeline, stage, activity, ticket, campaign, asset) embedded as JSON; registry + validate; SchemaViolation{path,msg}. Validation: `cargo test -p cdm` → ok then `echo m1: ok`. Expected: `m1: ok`. Recovery: jsonschema crate errors → print instance path in test failure message.
M2 identity resolution. Edits: deterministic keys per SPEC B4; MergeProposal{ids,confidence,evidence}. Validation: `cargo test -p cdm identity` green → `echo m2: ok`. Expected: `m2: ok`. Recovery: fixture-driven; add failing fixture as regress test before fixing.
M3 envelope machine. Edits: copy reference/governor.rs types; full transition table; Clock trait injected; transitions recorded in envelope.doc history vec. Validation: `cargo test -p governor envelope_` → `echo m3: ok`. Expected: `m3: ok`. Recovery: the transition match is exhaustive over the CURRENT state (no wildcard `from` arm; per-state `matches!` lists legal targets, terminal states return false) — adding a state must produce a compile error, and compile errors there are the guide.
M4 policy+constitution+evaluate. Edits: specificity resolve (exact(kind)>action>domain default; equal-specificity duplicates ⇒ load-time error), irreversible demotion, blast ceiling clamp, constitution (spend cap uses SpendSnapshot input, pii allowlist, hard_delete=false const). Validation: `cargo test -p governor` incl. proptests → `echo m4: ok`. Expected: `m4: ok`. Recovery: proptest failures print minimal case — turn each into a named regress test.
M5 perf assert. Edits: test `governor_eval_p99_under_5ms` building 10k random envelopes (seeded), asserting p99 duration; #[ignore] in debug, run with --release in test-unit? Simpler: guard with `#[cfg(not(debug_assertions))]`. Validation: `cargo test -p governor --release -- --include-ignored perf_` → `echo m5: ok`. Expected: `m5: ok`. Recovery: if slow, check no allocation in hot loop (resolve uses precomputed map).

## 9. Concrete Steps
As milestones; commit per milestone.

## 10. Validation and Acceptance
`bash scripts/test-unit.sh` → `unit tests: ok`; all SPEC-001 required tests present by name (`rg 'fn (prop_|regress_|perf_)' crates/governor crates/cdm` non-empty); diff ⊆ §6; `bash scripts/verify.sh` → `verify: ok`.

## 11. Idempotence and Recovery
Pure code; resume by running crate tests to locate first red.

## 12. Progress
- [ ] M1 - [ ] M2 - [ ] M3 - [ ] M4 - [ ] M5

## 13. Surprises & Discoveries
## 14. Decision Log
## 15. Outcomes & Retrospective
