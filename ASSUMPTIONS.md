# ASSUMPTIONS

| # | Assumption | Reason | Risk if wrong | How to verify | Blocks implementation? |
|---|---|---|---|---|---|
| A1 | Server-rendered shell (Axum+Askama+vendored htmx) is acceptable UI | No-Node constraint; solo-team velocity | UI expectations mismatch | VERIFIED-GREENFIELD: `ARCHITECTURE.md`, `DECISIONS.md` ADR-0003, and `MASTERPROMPT-INPUTS-FILLED.md` consistently target Axum + Askama + vendored htmx. | No — swap layer later; ADR-0003 |
| A2 | PostgreSQL 16 + NATS JetStream are the only stateful services | Minimal-dependency preference | Ops gap for queues | VERIFIED-GREENFIELD: `ARCHITECTURE.md`, `DECISIONS.md` ADR-0005, and `ENVIRONMENT.md` all name Postgres/NATS as the only stateful services. | No |
| A3 | DeepSeek cache granularity is 64-token blocks; usage exposes prompt_cache_hit_tokens / prompt_cache_miss_tokens | Vendor-documented behavior at design time | Hit-ratio math drifts | VERIFIED-GREENFIELD: `HOW-TO-USE.md`, `MASTERPROMPT-INPUTS-FILLED.md`, `TESTING.md`, and `reference/tokenkiller/ledger.rs` all assume 64-token blocks plus hit/miss usage fields; runtime probe remains EP-003 M4. | No — TK reads whatever usage fields exist via provider trait |
| A4 | Wasmtime component-model API is stable for WASI 0.2 worlds | Bridge runtime choice | Adapter ABI churn | VERIFIED-GREENFIELD: `ARCHITECTURE.md`, `SECURITY.md`, and `reference/bridge/*` consistently target Wasmtime component-model/WASI 0.2 as the only bridge host. | No |
| A5 | Legacy target for first bridge is SuiteCRM 7.x REST v8 sandbox | Free, self-hostable test target | First-bridge assumptions overfit | VERIFIED-GREENFIELD: `PROJECT_BRIEF.md`, `ENVIRONMENT.md`, and `TESTING.md` all point to a SuiteCRM-first bridge path. | No |
| A6 | Single-box: 8 vCPU / 32 GB serves ≤25 tenants | Sizing heuristic | Perf misses | VERIFIED-GREENFIELD: `PROJECT_BRIEF.md` and `MASTERPROMPT-INPUTS-FILLED.md` both carry the 8-vCPU/32GB ≤25-tenant sizing target. | No |
| A7 | Tenant admin = trusted operator (no hostile-tenant hardening in v1) | Scope control | Multi-tenant abuse | DELTA: only `ASSUMPTIONS.md` currently states this boundary; `SECURITY.md` does not restate it yet, so keep it as an unproven scope assumption pending later threat-model work. | No, but blocks SaaS |
| A8 | English-only UI v1 | Scope | i18n retrofit cost | DELTA: only `ASSUMPTIONS.md` currently states English-only v1; no separate spec or product doc repeats it yet. | No |

Rules: verify before relying; if an assumption fails, record in DECISIONS.md + active ExecPlan Decision Log, then adjust the spec — do not silently code around it.
