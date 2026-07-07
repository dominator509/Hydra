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

## ADR index
ADRs live inline below; new ADRs append using `.agent/templates/adr-template.md`.

### ADR-0006 TOKENKILLER (summary)
Context: agent loops dominate cost; DeepSeek prices cache-hit input ~an order cheaper; cache is longest-prefix, 64-token blocks; naive prompts (timestamps, shuffled JSON keys, rewritten history) massacre hit rates; runaway outputs ("nuclear failures") blow budgets and downstream parsers.
Decision: all LLM calls flow through tokenkiller (canonical serializer, stability-ordered segments S0–S3, append-only transcripts, block alignment, NukeGuard streaming budgets, output contracts, ledger with hit-ratio SLO 0.97).
Alternatives: per-agent ad-hoc prompting (rejected: unmeasurable), provider-side caching only (rejected: needs client discipline anyway), response max_tokens alone (rejected: doesn't stop dump patterns or repair).
Consequences: every prompt change to S0–S2 is a versioned event that intentionally resets cache; CI replay gate required; slight latency cost for canonicalization (<1ms).

## Rules for adding decisions
Any new dependency, schema change, ABI change, autonomy-cell default change, or S0–S2 prompt-segment change requires an ADR entry BEFORE merge.
