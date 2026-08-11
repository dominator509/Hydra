# EP-016 Nexus Model, A2A, and Skills

Plan status: DEFERRED

## 1. Purpose / Big Picture
Plan optional post-interoperability integration for a Nexus model provider, long-running A2A workflow facade, and signed Agent Skills discovery without weakening TOKENKILLER, Hydra governance, privacy, sandboxing, or standalone operation.

## 2. Scope
When and only when activated after EP-011 through EP-015 pass: optional `NexusModelProvider` inside llm-router behind TOKENKILLER; provider/privacy/budget provenance; local fallback; A2A only for long-running bridge/migration workflows; signed/versioned skill discovery and explicit trust/sandbox policy; no direct agent authority path.

## 3. Non-goals
No implementation while status is DEFERRED. No A2A for ordinary entity reads/basic mutations. No TOKENKILLER bypass or prompt/output-contract rewrite. No skill self-granted tools/credentials. No community skill execution without explicit trust/sandbox. No direct agent-to-agent mutation/approval path. No Hydra dependency on Nexus source. No production deployment.

## 4. Context and Orientation
EP-011 through EP-015 establish the only required v1 Nexus seam. This optional plan cannot begin until their index rows are `COMPLETE` with executable acceptance evidence. Hydra must continue using local providers and operate without Nexus.

## 5. Files to Read First
`.agent/state/execplan-index.md`; `.agent/specs/SPEC-009-tokenkiller.md`; `.agent/specs/SPEC-010-nexus-interoperability.md`; `ARCHITECTURE.md`; `SECURITY.md`; `crates/tokenkiller/src/session.rs`; `crates/llm-router/src/{lib.rs,routes.rs,providers/*}`; `crates/agents/src/*`; `crates/fabric/src/capabilities.rs`; bridge workflow/skill signing code that exists after EP-015. Re-read current official A2A/MCP/skill protocol sources at activation time.

## 6. Files to Change
Prospective only after activation: `DECISIONS.md`; `ARCHITECTURE.md`; `SECURITY.md`; `ENVIRONMENT.md`; `NEXUS_INTEGRATION.md`; a new accepted follow-up SPEC; `crates/llm-router/src/providers/nexus.rs` (new); `crates/llm-router/src/lib.rs`; `crates/tokenkiller/src/session.rs` only for provider provenance fields that preserve TK contracts; `crates/fabric/src/a2a.rs` (new); `crates/fabric/src/capabilities.rs`; `crates/agents/src/*` only for defined long-running workflow adapters; signed skill registry files discovered at activation; deterministic tests/fixtures; this plan and `.agent/state/execplan-index.md`. Exact paths MUST be reverified and amended in Decision Log before activation.

## 7. Interfaces and Contracts
Call path remains Hydra agent -> TOKENKILLER -> Hydra llm-router -> optional Nexus Model Gateway -> selected model. TOKENKILLER owns prompt construction, stability, NukeGuard, output contract, and ledger. Privacy/PII tags, budgets, and provider provenance round-trip. A2A exposes only long-running discovery/synthesis/conformance/wiring/canary/migration-assessment workflows and returns governed status/results. Skills are signed/versioned, declarative, least-authority, and cannot grant credentials/tools.

## 8. Milestones
M1 Activation audit and accepted spec. Validation: `bash scripts/check-execplan-state.sh`. Expected before activation: EP-011 through EP-015 are `COMPLETE`, EP-016 is the only `ACTIVE` plan. Recovery: any prerequisite not complete keeps EP-016 deferred.

M2 Nexus model provider behind TOKENKILLER. Validation: `cargo test -p llm-router nexus_provider -- --nocapture`. Expected: TK contracts/privacy/budgets/provenance/fallback tests pass. Recovery: provider unavailable -> local fallback/explicit unavailable, never direct router bypass.

M3 Long-running A2A facade. Validation: `cargo test -p fabric a2a_workflows -- --nocapture`. Expected: only allowlisted workflows, governed outputs, cancellation/resume, no basic CRUD path. Recovery: unsupported workflow is unavailable rather than generic execution.

M4 Signed skill interoperability. Validation: `cargo test -p agents skill_trust -- --nocapture`. Expected: signature/version/trust/sandbox/least-authority tests pass. Recovery: untrusted/unknown signer fails closed.

M5 Full security and verification. Validation: `bash scripts/security-check.sh` then `bash scripts/dependency-audit.sh` then `bash scripts/verify.sh`. Expected: all three `: ok` signals. Recovery: remove unsafe dependency/capability rather than weaken TOKENKILLER or governance.

## 9. Concrete Steps
Do nothing while deferred. At future activation, first re-read current repo/protocol/dependency state, create the required accepted spec, replace prospective paths with verified exact files, then execute M1-M5 in order with Decision Log updates.

## 10. Validation and Acceptance
Not applicable while deferred beyond verifying deferred state. Future completion requires all optional features to preserve TOKENKILLER, governance, privacy, fallback, standalone operation, and no direct authority path, with full security/dependency/verify gates green.

## 11. Idempotence and Recovery
Deferred state is safe and requires no runtime/config change. Future provider/A2A/skill registrations must reject duplicates, use stable IDs/versions, and be disableable. Failure returns to local providers/unavailable workflows without altering authoritative CRM state.

## 12. Progress
- [ ] M1 - DEFERRED: prerequisite audit and accepted spec
- [ ] M2 - DEFERRED: optional Nexus model provider
- [ ] M3 - DEFERRED: long-running A2A facade
- [ ] M4 - DEFERRED: signed skills interoperability
- [ ] M5 - DEFERRED: security/audit/verify

## 13. Surprises & Discoveries
None. Implementation is intentionally deferred.

## 14. Decision Log
| Date | Decision | Rationale |
|---|---|---|
| 2026-08-10 | Mark EP-016 DEFERRED until EP-011 through EP-015 pass | Optional model/A2A/skills work must not distract from or bypass the required secure interoperability seam |

## 15. Outcomes & Retrospective
Not executed. The correct current outcome is `DEFERRED`; no code/config/runtime behavior was changed for EP-016.
