# EP-016 Nexus Model, A2A, and Skills

Plan status: COMPLETE

## 1. Purpose / Big Picture
Plan optional post-interoperability integration for a Nexus model provider, long-running A2A workflow facade, and signed Agent Skills discovery without weakening TOKENKILLER, Hydra governance, privacy, sandboxing, or standalone operation.

## 2. Scope
EP-011 through EP-015 passed their executable acceptance gates, so this plan is
now activated. Implement the optional `NexusModelProvider` inside llm-router
behind TOKENKILLER; provider/privacy/budget provenance; local fallback; a
durable A2A 1.0 JSON-RPC facade only for long-running bridge/migration
workflows; and signed/versioned skill discovery with explicit trust and
sandbox policy. No direct agent authority path is added.

## 3. Non-goals
No implementation before activation. No A2A for ordinary entity reads/basic mutations. No TOKENKILLER bypass or prompt/output-contract rewrite. No skill self-granted tools/credentials. No community skill execution without explicit trust/sandbox. No direct agent-to-agent mutation/approval path. No Hydra dependency on Nexus source. No production deployment.

## 4. Context and Orientation
EP-011 through EP-015 establish the required v1 Nexus seam and are `COMPLETE`
in the authoritative index. The current official A2A 1.0 JSON-RPC binding and
the upstream Agent Skills format were re-read at activation; the latter does
not define signatures, so Hydra's Ed25519 manifest extension is explicitly
Hydra-local. Hydra must continue using local providers and operate without
Nexus.

## 5. Files to Read First
`.agent/state/execplan-index.md`; `.agent/specs/SPEC-009-tokenkiller.md`; `.agent/specs/SPEC-010-nexus-interoperability.md`; `ARCHITECTURE.md`; `SECURITY.md`; `crates/tokenkiller/src/session.rs`; `crates/llm-router/src/{lib.rs,routes.rs,providers/*}`; `crates/agents/src/*`; `crates/fabric/src/capabilities.rs`; bridge workflow/skill signing code that exists after EP-015. Re-read current official A2A/MCP/skill protocol sources at activation time.

## 6. Files to Change
`DECISIONS.md`; `ARCHITECTURE.md`; `SECURITY.md`; `ENVIRONMENT.md`;
`NEXUS_INTEGRATION.md`; `.agent/specs/SPEC-011-nexus-model-a2a-skills.md`;
`Cargo.toml`; `Cargo.lock`; `crates/llm-router/Cargo.toml`;
`crates/llm-router/src/providers/nexus.rs`; `crates/llm-router/src/lib.rs`;
`crates/tokenkiller/src/session.rs`; `crates/tokenkiller/src/ledger.rs`;
`crates/tokenkiller/src/lib.rs`; `crates/agents/Cargo.toml`;
`crates/agents/src/lib.rs`; `crates/agents/src/skills.rs`;
`crates/agents/tests/skill_trust.rs`; `crates/agents/tests/fixtures/skills/`;
`crates/store/src/lib.rs`; `crates/store/src/a2a_tasks.rs`;
`migrations/0014_a2a_tasks.sql`; `crates/fabric/Cargo.toml`;
`crates/fabric/src/lib.rs`; `crates/fabric/src/services.rs`;
`crates/fabric/src/rest/mod.rs`; `crates/fabric/src/rest/a2a.rs`;
`crates/fabric/tests/a2a_workflows.rs`; `crates/kernel/src/config.rs`;
`crates/kernel/src/main.rs`; `crates/kernel/src/runtime_services.rs`;
`crates/kernel/tests/runtime_wiring.rs`;
`.env.example`; `docker/nexus.env.example`; `NEXUS_PACKAGE_CONTRACT.md`;
`TESTING.md`; this plan; `.agent/state/execplan-index.md`; and checked
`.sqlx/` metadata; `scripts/check-execplan-state.sh`.

## 7. Interfaces and Contracts
Call path remains Hydra agent -> TOKENKILLER -> Hydra llm-router -> optional
Nexus Model Gateway -> selected model. TOKENKILLER owns prompt construction,
stability, NukeGuard, output contract, and ledger. Privacy/PII tags, budgets,
and provider provenance round-trip. A2A 1.0 JSON-RPC exposes durable,
tenant-scoped tasks for only the allowlisted long-running workflows and
returns governed status/results. Skills use the upstream `SKILL.md` format
plus Hydra's signed Ed25519 manifest, are declarative and least-authority, and
cannot grant credentials/tools.

## 8. Milestones
M1 Activation audit and accepted spec. Validation: `bash scripts/check-execplan-state.sh`. Expected: `execplan state: ok`; EP-011 through EP-015 are `COMPLETE`, EP-016 is the only `ACTIVE` plan, and SPEC-011 exists. Recovery: repair the index/spec atomically and rerun; do not implement runtime code while the state checker is red.

M2 Nexus model provider behind TOKENKILLER. Validation: `cargo test -p llm-router nexus_provider -- --nocapture`. Expected: provider request contract, TK-only path, privacy/budget/provenance, redacted failure, and local fallback tests pass. Recovery: provider unavailable -> local fallback/explicit unavailable, never direct router bypass.

M3 Long-running A2A facade. Validation: `cargo test -p fabric a2a_workflows -- --nocapture`. Expected: A2A 1.0 JSON-RPC validation, authenticated tenant scoping, durable message-id idempotency, cancellation/resume, and allowlisted workflow truth pass; no basic CRUD path. Recovery: unsupported workflow is unavailable rather than generic execution.

M4 Signed skill interoperability. Validation: `cargo test -p agents skill_trust -- --nocapture`. Expected: upstream metadata, Ed25519 manifest signature/hash, version/trust/key-rotation, sandbox, and least-authority tests pass. Recovery: untrusted/unknown signer fails closed.

M5 Full security and verification. Validation: `bash scripts/security-check.sh` then `bash scripts/dependency-audit.sh` then `bash scripts/verify.sh`. Expected: all three `: ok` signals. Recovery: remove unsafe dependency/capability rather than weaken TOKENKILLER or governance.

## 9. Concrete Steps
Execute M1 through M5 in order. Use the existing provider and Store patterns;
do not add a new protocol or authority path. Keep unsupported workflow and
skill capabilities unavailable rather than fabricating runtime support.

## 10. Validation and Acceptance
Completion requires all optional features to preserve TOKENKILLER, governance,
privacy, fallback, standalone operation, durable tenant scoping, and no direct
authority path, with full security/dependency/verify gates green. Production
readiness remains partial if EP-010 staging and human-owned evidence is absent.

## 11. Idempotence and Recovery
Activation is safe and additive; future provider/A2A/skill registrations must reject duplicates, use stable IDs/versions, and be disableable. Failure returns to local providers/unavailable workflows without altering authoritative CRM state.

## 12. Progress
- [x] M1 - Activation audit and accepted spec (`SPEC-011` accepted; state checker passed)
- [x] M2 - Optional Nexus model provider (`cargo test -p llm-router nexus_provider -- --nocapture` -> 3 passed)
- [x] M3 - Long-running A2A facade (`cargo test -p fabric a2a_workflows -- --nocapture` -> 1 passed; wider `a2a` filter -> 4 passed)
- [x] M4 - Signed skills interoperability (`cargo test -p agents skill_trust -- --nocapture` -> 3 passed; Kernel runtime-wiring filter -> 4 passed)
- [x] M5 - Security/audit/verify (`bash scripts/security-check.sh` -> `security check: ok`; `bash scripts/dependency-audit.sh` -> `dependency audit: ok`; explicit loopback `bash scripts/verify.sh` -> exit 0 through terminal `verify: ok`; 2026-08-11)

## 13. Surprises & Discoveries
Activation re-read the current A2A 1.0 JSON-RPC binding and upstream Agent
Skills format. Agent Skills defines `SKILL.md` packaging but no signature
standard; Hydra therefore uses a separate Ed25519 JWS manifest and does not
pretend that unsigned upstream packages are trusted. The implementation keeps
A2A streaming/push unavailable because no durable worker/notification path is
present yet.

## 14. Decision Log
| Date | Decision | Rationale |
|---|---|---|
| 2026-08-10 | Mark EP-016 DEFERRED until EP-011 through EP-015 pass | Optional model/A2A/skills work must not distract from or bypass the required secure interoperability seam |
| 2026-08-11 | Activate EP-016 after EP-011 through EP-015 completed | The prerequisite rows have executable acceptance evidence; the optional seam can now be implemented without changing the v1 trust boundary |
| 2026-08-11 | Use A2A 1.0 JSON-RPC with streaming and push explicitly unavailable | The current official protocol defines PascalCase JSON-RPC methods and durable task lifecycle; Hydra has no safe streaming/push worker yet, so capability truth must fail closed |
| 2026-08-11 | Use an OpenAI-compatible Nexus gateway adapter and a Hydra-local Ed25519 skill manifest | Reuse the existing provider HTTP contract and locked JWT crypto path; the upstream Agent Skills format does not standardize signatures |
| 2026-08-11 | Keep signed skill discovery declarative-only and wire it through RuntimeServices | Skills must not execute scripts, grant credentials, or create a new authority path; absent configuration remains valid standalone behavior, while partial or invalid configuration fails closed |
| 2026-08-11 | Run M5 against explicit loopback test services | Ambient `localhost:5432` credentials and an absent NATS listener made the first verifier attempt invalid; rerunning against `127.0.0.1:55432` and a disposable local JetStream listener passed without targeting production infrastructure |

## 15. Outcomes & Retrospective
M1 through M5 are complete. The Nexus provider is TOKENKILLER-only with
privacy/provenance/budget enforcement and local fallback; A2A is durable,
tenant-scoped, idempotent, resumable, and explicitly allowlisted; and signed
skill discovery validates upstream metadata, Ed25519/hash claims, key
rotation, least authority, and declarative sandbox policy. Kernel wires the
optional registry and reports its availability without exposing skill
execution. The explicit security, dependency, and full-verification gates
passed against loopback-only test infrastructure. A2A streaming/push and skill
execution remain intentionally unavailable rather than being falsely exposed;
EP-010 production readiness remains partial and no production deployment or
database operation occurred.

## Post-EP-033 Current Verification (2026-08-12)

EP-033 extends the existing A2A boundary with one authenticated,
proposal-only `bridge-synthesis` workflow backed by TOKENKILLER mapping
contracts. EP-016's model gateway, skill discovery, A2A protocol, and
streaming/push non-goals remain unchanged; synthesis cannot activate a bridge
or grant authority. The new workflow is an additive follow-on and does not
change EP-010's partial production-readiness status.
