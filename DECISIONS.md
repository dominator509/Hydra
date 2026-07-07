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

## Rules for adding decisions
Any new dependency, schema change, ABI change, autonomy-cell default change, or S0–S2 prompt-segment change requires an ADR entry BEFORE merge.
