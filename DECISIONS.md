# DECISIONS.md — Architecture Decision Log

## Decision table
| ADR | Title | Status | Date | Owner |
|---|---|---|---|---|
| 0001 | Rust workspace, 6-layer import law | Accepted | 2026-07-06 | djw |
| 0002 | WASM Component Model (WIT `hydra:bridge@1.0.0`) as the ONLY adapter ABI | Accepted | 2026-07-06 | djw |
| 0003 | Server-rendered shell: Axum+Askama+vendored htmx; zero Node | Accepted | 2026-07-06 | djw |
| 0004 | Deterministic Governor; LLMs propose, code disposes | Accepted | 2026-07-06 | djw |
| 0005 | Postgres 16 + NATS JetStream as only stateful services; outbox pattern | Accepted | 2026-07-06 | djw |
| 0006 | TOKENKILLER mandatory call path; DeepSeek prefix-cache discipline, ≥97% target; NukeGuard on all streams | Accepted | 2026-07-06 | djw |
| 0007 | Fixed transform library in sync path; no agent-authored code executes there | Accepted | 2026-07-06 | djw |
| 0008 | Soft-delete only; append-only audit | Accepted | 2026-07-06 | djw |
| 0009 | Foundation workspace kernel uses Axum/Tokio/Tracing/Thiserror on Rust 1.96.1 | Accepted | 2026-07-07 | Codex |
| 0010 | EP-002 core-domain crates use jsonschema, time, and proptest | Accepted | 2026-07-07 | Codex |
| 0011 | EP-003 persistence/runtime uses sqlx offline metadata and async-nats relay plumbing | Accepted | 2026-07-07 | Codex |
| 0012 | EP-003 SQLx audit hardening uses a repo-local Postgres-only vendor patch | Accepted | 2026-07-07 | Codex |

## ADR index
ADRs live inline below; new ADRs append using `.agent/templates/adr-template.md`.

### ADR-0006 TOKENKILLER (summary)
Context: agent loops dominate cost; DeepSeek prices cache-hit input ~an order cheaper; cache is longest-prefix, 64-token blocks; naive prompts (timestamps, shuffled JSON keys, rewritten history) massacre hit rates; runaway outputs ("nuclear failures") blow budgets and downstream parsers.
Decision: all LLM calls flow through tokenkiller (canonical serializer, stability-ordered segments S0–S3, append-only transcripts, block alignment, NukeGuard streaming budgets, output contracts, ledger with hit-ratio SLO 0.97).
Alternatives: per-agent ad-hoc prompting (rejected: unmeasurable), provider-side caching only (rejected: needs client discipline anyway), response max_tokens alone (rejected: doesn't stop dump patterns or repair).
Consequences: every prompt change to S0–S2 is a versioned event that intentionally resets cache; CI replay gate required; slight latency cost for canonicalization (<1ms).

### ADR-0009 Foundation runtime/toolchain set
Context: EP-001 needs a real Rust workspace, a binary kernel that can serve `/healthz`, and repo-level lints/scripts that compile on this machine without pulling in an unnecessary frontend or service stack before later plans define behavior. AGENTS.md also requires a durable ADR entry for new dependencies before merge.
Decision: use Rust 1.96.1 (already installed on this host) as the pinned workspace toolchain and keep the initial runtime surface minimal: `axum` for the health endpoint/router, `tokio` for async runtime + socket binding, `tracing` + `tracing-subscriber` for structured logs, and `thiserror` for kernel-local errors. Keep the remaining crates dependency-free placeholders until later ExecPlans justify more imports.
Alternatives: raw `hyper`/manual HTTP server (rejected: less aligned with the accepted Axum shell direction), adding broader deps up front such as `askama`, `sqlx`, or `reqwest` in M1 (rejected: unnecessary before later plans define the behavior), pinning an older toolchain than the installed one (rejected: would likely trigger an avoidable network toolchain install here).
Consequences: M1 compiles the workspace with a minimal binary and empty layer placeholders, future plans can add crate-specific deps incrementally with their own ADR coverage, and the workspace stays close to the accepted architecture without over-materializing later-plan behavior.

### ADR-0010 EP-002 core-domain dependency set
Context: EP-002 needs schema-bound entity validation, RFC3339 transition timestamps, and property-based safety tests in the pure L1 crates while still obeying the repo's layer law and AGENTS.md's requirement that new dependencies get a durable ADR entry before merge.
Decision: add `jsonschema` to `crates/cdm` for the builtin kind registry, `time` to `crates/governor` for RFC3339 timestamp formatting in transition history, and `proptest` as a shared dev-dependency for `cdm` and `governor` property/regression coverage. Keep the rest of the implementation on the already accepted `serde`, `serde_json`, `uuid`, and `thiserror` stack.
Alternatives: hand-roll JSON Schema checks (rejected: slower, less correct, and would duplicate a mature validator), use `chrono` for timestamps (rejected: unnecessary when `time` cleanly covers the RFC3339 formatting need), and rely only on example-based tests (rejected: weaker coverage for the matrix-resolution and state-machine invariants that SPEC-001 explicitly calls out for property tests).
Consequences: EP-002 can enforce kind schemas and pure-governor invariants locally with green `cargo test` / `verify.sh` gates, later plans inherit a stable L1 API surface, and the workspace lockfile expands accordingly under the repo's normal audit flow.

### ADR-0011 EP-003 persistence/runtime dependency set
Context: EP-003 adds the durable Postgres spine, checked repository queries, and the first kernel-side outbox relay to NATS while AGENTS.md still requires durable ADR coverage before any new dependency set is merged.
Decision: add workspace-level `sqlx` with the Postgres/runtime/macros/migrate/uuid/json/time feature set for migrations, checked queries, and offline `.sqlx/` metadata; add workspace-level `async-nats` for the kernel relay and readiness ping; expand shared `tokio` features to include `signal` and `sync` so the kernel can drive graceful shutdown and watch-based relay coordination.
Alternatives: use `tokio-postgres` plus hand-written row mapping (rejected: loses the repo's compile-time query contract and migration tooling), defer offline metadata and rely on a live `DATABASE_URL` for every compile (rejected: breaks the repo's documented `verify.sh` surface), or introduce a larger messaging abstraction before the first relay exists (rejected: unnecessary before later plans consume NATS subjects).
Consequences: EP-003 can enforce schema/query compatibility through committed `.sqlx/` snapshots, local DB setup remains the single source of truth for preparing queries, and the kernel now depends only on the already accepted Postgres+NATS stateful services when proving readiness and relaying outbox events.

### ADR-0012 EP-003 SQLx audit hardening via repo-local vendor patch
Context: `cargo audit` stayed red after the EP-003 persistence work because SQLx 0.8.6 still pulled `rsa` into `Cargo.lock` through optional `sqlx-mysql` metadata, triggering `RUSTSEC-2023-0071` even though Hydra only enables the Postgres feature set and `cargo tree --target all -i rsa` showed no active runtime edge. AGENTS.md and the repo security gates require a green `cargo audit` inside `bash scripts/verify.sh`.
Decision: replace the workspace `sqlx` dependency with a repo-local Postgres-only facade under `vendor/sqlx` and patch `sqlx-macros-core` under `vendor/sqlx-macros-core` so the lockfile no longer carries optional MySQL or SQLite packages that Hydra never ships. Keep the public SQLx macro/runtime surface Hydra already uses so the store and kernel code remain unchanged above the dependency boundary.
Alternatives: add a repo-local audit ignore for `RUSTSEC-2023-0071` (rejected: hides a red gate instead of shrinking the dependency surface), replace SQLx entirely with a different Postgres stack inside EP-003 (rejected: far too large a drift from the spec and the already-working checked-query path), or wait for an upstream SQLx fix (rejected: no fixed release was available and EP-003 needed a green verify gate now).
Consequences: `cargo audit`, `cargo deny`, and `bash scripts/verify.sh` are green again while Hydra still uses SQLx's checked-query workflow, but the repo now owns a small vendor patch set that should be retired once upstream SQLx ships an audit-clean equivalent. The vendored `sqlx-macros-core` copy also carries warning-only `unexpected_cfgs` noise that is acceptable for now but worth cleaning up when the vendor patch is revisited.

## Rules for adding decisions
Any new dependency, schema change, ABI change, autonomy-cell default change, or S0–S2 prompt-segment change requires an ADR entry BEFORE merge.
