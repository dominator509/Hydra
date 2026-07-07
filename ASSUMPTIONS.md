# ASSUMPTIONS

| # | Assumption | Reason | Risk if wrong | How to verify | Blocks implementation? |
|---|---|---|---|---|---|
| A1 | Server-rendered shell (Axum+Askama+vendored htmx) is acceptable UI | No-Node constraint; solo-team velocity | UI expectations mismatch | Review SPEC-004 wireflows with owner | No — swap layer later; ADR-0003 |
| A2 | PostgreSQL 16 + NATS JetStream are the only stateful services | Minimal-dependency preference | Ops gap for queues | docker-compose up; smoke test | No |
| A3 | DeepSeek cache granularity is 64-token blocks; usage exposes prompt_cache_hit_tokens / prompt_cache_miss_tokens | Vendor-documented behavior at design time | Hit-ratio math drifts | EP-003 M4 probe: send twin requests, diff usage fields; record in Decision Log | No — TK reads whatever usage fields exist via provider trait |
| A4 | Wasmtime component-model API is stable for WASI 0.2 worlds | Bridge runtime choice | Adapter ABI churn | Pin wasmtime version in Cargo.toml; conformance suite | No |
| A5 | Legacy target for first bridge is SuiteCRM 7.x REST v8 sandbox | Free, self-hostable test target | First-bridge assumptions overfit | EP-006 canary vs a second adapter (CSV/SQL) | No |
| A6 | Single-box: 8 vCPU / 32 GB serves ≤25 tenants | Sizing heuristic | Perf misses | EP-010 load smoke (k6-free: hey/curl loop) | No |
| A7 | Tenant admin = trusted operator (no hostile-tenant hardening in v1) | Scope control | Multi-tenant abuse | SECURITY.md threat model review before public offering | No, but blocks SaaS |
| A8 | English-only UI v1 | Scope | i18n retrofit cost | Owner confirm | No |

Rules: verify before relying; if an assumption fails, record in DECISIONS.md + active ExecPlan Decision Log, then adjust the spec — do not silently code around it.
