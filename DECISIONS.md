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
| 0013 | EP-004 bridge ABI M1 uses wit-bindgen plus a workspace fixture adapter build path | Accepted | 2026-07-07 | Codex |
| 0014 | EP-004 bridge-host uses Wasmtime 38 bindgen flags, scoped unsafe, and repo-local store bridging | Accepted | 2026-07-07 | Codex |
| 0015 | EP-004 TOKENKILLER core uses canonicalization/hash deps plus local router and ledger seams | Accepted | 2026-07-07 | Codex |
| 0016 | EP-004 llm-router M5 uses YAML route loading, provider fakes, and direct TOKENKILLER integration | Accepted | 2026-07-07 | Codex |
| 0017 | EP-004 TOKENKILLER M6 uses warm replay fixtures and a self-bootstrapping cache-hit audit gate | Accepted | 2026-07-07 | Codex |
| 0018 | EP-004 M7 starts with fabric-local service traits, problem+json routes, and a store-backed executor seam | Accepted | 2026-07-07 | Codex |
| 0019 | Hydra is the CRM/revenue bounded context beneath Nexus; integration is authenticated MCP, REST, and durable events | Accepted | 2026-08-10 | Codex |
| 0020 | Pin the bridge sandbox to Wasmtime 36.0.13 LTS and accept Cranelift's LLVM exception | Accepted | 2026-08-10 | Codex |
| 0021 | Adopt the official rmcp 3.1.2 SDK for the MCP 2025-11-25 Streamable HTTP boundary | Accepted | 2026-08-10 | Codex |
| 0022 | Validate Nexus access tokens with pinned jsonwebtoken 9.3.1 and Hydra-owned external bindings | Accepted | 2026-08-10 | Codex |
| 0023 | Use acknowledged async-nats JetStream publication with Store-owned outbox leases | Accepted | 2026-08-11 | Codex |
| 0033 | Store-owned tenant export and non-destructive retention preview | Accepted | 2026-08-11 | Codex |
| 0024 | Keep W3C trace carriers separate from durable business provenance and gate Nexus readiness on event infrastructure | Accepted | 2026-08-11 | Codex |
| 0025 | Use the already-locked futures-util stream extension only in the fake Nexus consumer test | Accepted | 2026-08-11 | Codex |
| 0026 | Use existing reqwest/jsonwebtoken graph only in the fake Nexus HTTP/JWT harness | Accepted | 2026-08-11 | Codex |
| 0027 | Use a pinned age vault and runtime SecretSource boundary for named adapter secrets | Accepted | 2026-08-11 | Codex |
| 0028 | Keep optional Nexus model, A2A, and signed skills seams bounded and non-authoritative | Accepted | 2026-08-11 | Codex |
| 0029 | Govern prebuilt bridge lifecycle through Store, BridgeHost, and typed execution handlers | Accepted | 2026-08-11 | Codex |
| 0032 | Make explicit outbound proxy configuration mandatory in staging and production | Accepted | 2026-08-11 | Codex |
| 0036 | Use Store-backed atomic windows as the distributed rate-limit authority | Accepted | 2026-08-11 | Codex |

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

### ADR-0013 EP-004 bridge ABI M1 binding/tooling set
Context: EP-004 M1 needs the normative `hydra:bridge@1.0.0` WIT world checked into `wit/`, Rust guest bindings for both imported host calls and exported adapter traits, and a hand-written fixture adapter that can be deterministically rebuilt into `adapters/memcrm.wasm` on this Windows machine.
Decision: pin `wit-bindgen` 0.57.1 in `crates/bridge-wit`, keep the normative WIT file at `wit/hydra-bridge.wit`, and add `fixtures/adapter-memcrm` as a workspace fixture crate built through `bash scripts/build-adapters.sh` by staging the already-componentized `wasm32-wasip2` artifact directly into `adapters/memcrm.wasm`. Require the Rust `wasm32-wasip2` target explicitly instead of inventing a custom adapter build flow.
Alternatives: hand-write host/guest ABI glue without `wit-bindgen` (rejected: too error-prone for the repo's normative ABI), defer the fixture adapter until Wasmtime host work lands (rejected: EP-004 M1 explicitly requires an adapter artifact before M2/M3), or keep the fixture crate outside the workspace with a separate lockfile (rejected: weaker reproducibility and a messier repo contract).
Consequences: Hydra gets a single-source-of-truth WIT contract plus a repeatable local adapter artifact path, while the repo takes on one new pinned binding dependency and one extra toolchain prerequisite (`rustup target add wasm32-wasip2`). Later EP-004 milestones can build the host and conformance layers on top of the same checked-in ABI without re-deriving it.

### ADR-0014 EP-004 bridge-host binding/runtime shape
Context: EP-004 M2 needs `bridge-host` to instantiate the Rust-built fixture component through `wasmtime 38.0.4`, expose the WIT host surface asynchronously, satisfy the fixture adapter's WASI preview2 imports, persist adapter KV state via `store`, and keep the Wasmtime boundary isolated to one crate under Hydra's architecture rules. The current Wasmtime macro surface differs from the older reference sketch, and the generated bindings emit internal `unsafe` blocks that conflict with the workspace-wide forbid lint.
Decision: add `wasmtime`, `wasmtime-wasi`, `reqwest`, `async-trait`, and `anyhow` to the workspace for the bridge-host seam; configure `crates/bridge-host` bindings with `imports: { default: async | trappable }` and `exports: { default: async }`; store a `WasiCtx` plus `ResourceTable` inside `HostState` and implement `WasiView` so `wasmtime_wasi::p2::add_to_linker_async` can satisfy the standard preview2 resource world; use `wasmtime::component::HasSelf<HostState>` plus a marker `types::Host` impl to satisfy the generated linker API; and scope `unsafe_code = "allow"` to `crates/bridge-host` only while preserving `clippy::unwrap_used = "deny"` there. Keep adapter scratch state persisted through `crates/store/src/adapter_kv.rs` rather than adding SQL in bridge-host.
Alternatives: keep the older `bindgen!({ async: true })` reference syntax (rejected: invalid for Wasmtime 38), rewrite `AdapterKvRepo` calls around ad hoc host-local maps (rejected: violates the intended persistence seam), or relax the workspace unsafe lint globally (rejected: far broader than the single Wasmtime boundary Hydra already isolates).
Consequences: the repo now has a truthful bridge-host runtime surface for Wasmtime 38, preview2 WASI resources, and adapter KV storage, and the EP-004 M2 validation target passes against the real Rust-built fixture component. The unsafe exception stays local to the one crate that already owns the Wasmtime trust boundary.

### ADR-0015 EP-004 TOKENKILLER core dependency and seam shape
Context: EP-004 M4 needs canonical prompt bytes that match SPEC-009/reference behavior, stable prefix hashes for cache accounting, output containment and repair-once enforcement, durable ledger writes into `store`, and a real `Session::complete` call path before M5's concrete provider/router implementation exists. `crates/tokenkiller` started this milestone as a placeholder only, and AGENTS.md requires durable ADR coverage for the new dependency set before merge.
Decision: add workspace `ryu`, `sha2`, and `unicode-normalization`, then use `async-trait`, `serde_json`, `store`, `time`, `uuid`, and `thiserror` inside `crates/tokenkiller` with `proptest` and `tokio` as dev-dependencies. Implement TOKENKILLER as six internal modules (`canon`, `prefix`, `nukeguard`, `contracts`, `ledger`, and `session`) and keep `Session` generic over local async `Router` and `LedgerSink` traits, with `StoreLedgerSink` bridging persisted usage into `store::LedgerRepo` until EP-004 M5 lands the concrete provider stack.
Alternatives: wait for M5 and build TOKENKILLER only after `llm-router` is real (rejected: violates milestone order and leaves Hydra without its only permitted LLM call path), couple M4 directly to the placeholder `llm-router` crate (rejected: would fake an unfinished dependency surface), or keep the crate placeholder-green with zero tests (rejected: makes the milestone validation meaningless).
Consequences: Hydra now has a real TOKENKILLER core with deterministic canonicalization, stable prefix hashing, append-only transcript support, budget enforcement, contract repair-once behavior, and persistent ledger math that can be reused by the later provider layer. The router/provider implementation remains decoupled, so M5 can plug real providers into `Session` without rewriting the M4 public seam.

### ADR-0016 EP-004 llm-router M5 dependency and integration shape
Context: EP-004 M5 needs a real multi-provider router with ordered route chains, structural PII gating, provider-specific usage parsing, cost estimates, and a cache-aware DeepSeek fake for tests. At the same time, `crates/fabric` is still a placeholder, and M4 already established `tokenkiller::Session` as the only allowed LLM call path with a local router trait.
Decision: add local `serde_yaml` and dev-only `wiremock` to `crates/llm-router`; keep `reqwest` behind a small local `JsonHttpClient` inside the crate for now; implement YAML-backed `RouteCfg` loading plus provider modules for Anthropic, DeepSeek, and OpenAI-compatible endpoints; have `llm-router::Router` implement `tokenkiller::Router` directly; and extend `tokenkiller::CompletionResponse` with the actual responding provider so fallback winners land truthfully in the ledger.
Alternatives: wait for M7 to build `fabric::egress` before landing any real router/provider code (rejected: violates milestone order and blocks M6 replay work), call `reqwest` ad hoc from each provider without a shared client seam (rejected: harder to swap or audit later), or keep the ledger provider sourced only from the route's preferred provider name (rejected: incorrect as soon as fallback or degradation takes effect).
Consequences: Hydra now has a real llm-router crate with deterministic route loading, chain traversal, structural PII enforcement, provider-aware accounting, and a reusable DeepSeek cache fake that M6 can drive through TOKENKILLER. The egress seam remains local and easy to replace once Fabric's broader surface exists, and the router now composes with `Session::complete` without another shared-crate extraction step.

### ADR-0017 EP-004 TOKENKILLER M6 corpus and replay gate shape
Context: EP-004 M6 needs a real replay corpus that proves DeepSeek-style prefix reuse through `Session::complete`, a scriptable ratio gate for `verify.sh`, and per-call forensics when the ratio dips. The new M5 router already provides a cache-aware fake, but cold-start calls and missing `DATABASE_URL` bootstrap would make the bare audit command fail or under-report steady-state behavior.
Decision: add three JSON fixtures under `tests/fixtures/tk-corpus/` plus `crates/tokenkiller/tests/replay_corpus.rs`, with `llm-router`, `wiremock`, and `serde` as tokenkiller dev-dependencies. Drive one warm-up call per route to seed the fake, then measure the next fourteen append-only transcript turns per route, logging every measured `prefix_sha` plus hit/miss pair and printing a final `tk-corpus ratio: ...` line for `scripts/cache-hit-audit.sh`. Make the script default `DATABASE_URL` to the repo’s documented local Postgres example when the variable is unset so the gate can run under `verify.sh` without extra shell wrapping.
Alternatives: measure cold starts directly (rejected: the three-route corpus cannot reach the required `>=0.97` ratio if cold misses count), replace the router fake with an in-test stub that bypasses M5’s real provider/accounting path (rejected: weaker evidence), or require the caller to export `DATABASE_URL` manually for the audit script (rejected: too brittle for a command wired into `verify.sh`).
Consequences: Hydra now has a truthful TK replay gate that exercises the real router/session seam, catches unstable prefix bytes through logged `prefix_sha` transitions, and proves the steady-state cache economics target with a local command. The audit remains local-only and still inherits the broader repo’s separate `cargo deny` license-policy blocker.

### ADR-0018 EP-004 M7 initial service/executor seam
Context: EP-004 M7 needs the first real fabric REST/MCP surface plus the kernel execution path behind `Decision::Execute`, but the milestone is broad and `scripts/test-integration.sh` validates through integration targets rather than unit-only seams. The repo also already exposes `store` and `governor` primitives that can support a truthful first slice without inventing later provider or shell behavior.
Decision: start M7 with fabric-local service traits backed by real `store` and `governor` implementations, problem+json HTTP error mapping, a compact MCP schema export, and a store-backed kernel executor that loads approved envelopes by id and applies `pipeline/move_stage` mutations to `deal` entities. Add integration tests in `crates/fabric/tests/integration_contracts.rs` and `crates/kernel/tests/integration_executor.rs` so the first REST/MCP and executor seams are validated with real Postgres-backed repos. Wire the needed crate manifests directly in `crates/fabric/Cargo.toml` and `crates/kernel/Cargo.toml`, and extend `store::EnvelopesRepo` with a narrow `get_by_id` lookup rather than broadening the executor token contract.
Alternatives: wait to land M7 only as one large all-or-nothing patch (rejected: too much surface to validate truthfully in one step), invent a wider shared service crate before the first routes exist (rejected: unnecessary drift from the current architecture), or fake the executor path in unit tests without durable store reads (rejected: weaker evidence for `Decision::Execute`).
Consequences: Hydra now has a real first M7 slice that can be committed and pushed independently while keeping the milestone open. The repo gains more direct integration coverage and a narrow store lookup seam, and later M7 work can extend the REST surface, receipts/events, and `concierge.ping` path without rewriting the initial contract layer.

### ADR-0019 Hydra as the CRM/revenue bounded context beneath Nexus
Context: Nexus is a larger whole-life and executive AI control plane, while Hydra already owns the canonical CRM model, identity resolution, CRM bridges, synchronization, conflict policy, action governance, audit, and execution. The existing Hydra runtime exposes development-grade REST/MCP/auth and coarse NATS publication that cannot safely establish tenant authority or durable interoperability. Adding a second CRM abstraction in Nexus or allowing direct database/vendor access would split authority and bypass Hydra's strongest controls.

Decision: Hydra remains the independently deployable CRM/revenue bounded context and canonical CRM source of truth. Nexus integrates only through authenticated, versioned MCP Streamable HTTP, a narrow `/v1/nexus/` REST facade, and durable canonical events over NATS JetStream. Fabric translates Nexus claims into generic principal, binding, capability, and invocation types. An explicit Hydra-owned external business binding resolves the Hydra tenant; request metadata cannot choose tenant authority. Nexus authorization and Hydra Governor form independent mandatory gates for every external mutation. Nexus stores entity references or projections, never a writable duplicate CRM. GraphQL is deferred and is not required for Nexus v1.

Alternatives: direct Nexus access to Hydra Postgres (rejected: bypasses tenant, audit, and store boundaries); Nexus calls to vendor CRM APIs (rejected: bypasses bridges, CDM, and conflict policy); a Nexus-specific CRM model inside Hydra L1/L2 (rejected: contaminates the bounded context); unrestricted generic execute tools (rejected: bypasses typed capabilities and Governor); synchronous callback to Nexus on every request (rejected: couples availability and latency); GraphQL for v1 (rejected: no current implementation and no interoperability need).

Consequences: Hydra must add asymmetric JWT resource-server validation, explicit external tenant/business bindings, one typed capability registry, governed proposal-only external mutations, durable provenance/idempotency, typed execution handlers, a versioned event envelope, JetStream publish acknowledgements, and contract/E2E tests with a fake Nexus harness. Standalone mode remains supported. EP-010 remains partial until its staging and human-owned readiness evidence is actually performed. No production deployment is authorized by this decision.

### ADR-0020 Wasmtime 36.0.13 LTS security baseline
Context: EP-011's first full current `cargo audit` found 18 vulnerabilities in `wasmtime` and `wasmtime-wasi` 38.0.4, including `RUSTSEC-2026-0095` and `RUSTSEC-2026-0096` critical sandbox-escape advisories. The strictest fixed-version requirement is 36.0.13 or 47.0.3. Wasmtime's official release policy identifies 36.x as the current LTS, supports it for 24 months, guarantees security backports, and guarantees patch-version API compatibility. Current `cargo deny` also rejects the Cranelift dependency family's upstream `Apache-2.0 WITH LLVM-exception` SPDX expression because Hydra allowed only plain Apache-2.0. Wasmtime 36.0.13 transitively includes the unmaintained `fxhash` 0.2.1 through its optional profiling support, and `webpki-roots` 1.0.8 declares `CDLA-Permissive-2.0` for Mozilla's curated CA root data.

Decision: pin workspace `wasmtime` and `wasmtime-wasi` exactly to 36.0.13, the smallest current LTS release that satisfies every observed RustSec remediation floor. Keep the existing async Component Model/WASI architecture and conformance surface unchanged. Add `Apache-2.0 WITH LLVM-exception` to the license allowlist because it is the declared license of the required Cranelift implementation under the accepted Wasmtime-only adapter ABI. Keep direct unmaintained dependencies denied by setting `unmaintained = "workspace"`; this classifies transitive `fxhash` as informational without suppressing it from `cargo audit`. Add a crate-and-version-scoped license exception for `webpki-roots@1.0.8` and its `CDLA-Permissive-2.0` data license rather than allowing that license globally. Do not add advisory ignores.

Alternatives: remain on 38.0.4 and ignore advisories (rejected: leaves known critical sandbox escapes in Hydra's only adapter boundary); jump to 47.0.3 (rejected for this baseline repair: a non-LTS major with larger API and default-feature changes); disable Wasmtime/Cranelift (rejected: violates ADR-0002 and removes Hydra's only permitted legacy adapter ABI); globally allow all unmaintained transitive dependencies or `CDLA-Permissive-2.0` (rejected: broader than the evidenced exceptions); add a RustSec ignore for `fxhash` (rejected: hides useful audit evidence rather than classifying its transitive maintenance status).

Consequences: the bridge runtime stays on a security-supported LTS line through August 2027 and must pass its real component/conformance tests plus `cargo audit` and `cargo deny check`. `cargo audit` continues to report transitive maintenance/unsoundness warnings for owner review while the deny gate fails on vulnerable, yanked, or directly unmaintained dependencies. The root-certificate data license exception is exact to `webpki-roots` 1.0.8. ADR-0014's version-specific implementation history remains preserved, but 36.0.13 is the current dependency authority. Future Wasmtime changes remain explicit ADR decisions and may not bypass the security gates.

### ADR-0021 Official Rust MCP SDK boundary
Context: EP-012 requires MCP `2025-11-25` Streamable HTTP conformance, protocol negotiation, Host/Origin defenses, request-size limits, cancellation, deterministic schemas, and request-scoped authenticated identity. Hydra's hand-written JSON-RPC route advertises a non-standard `1.0` protocol, accepts tenant authority from tool metadata, and would require reimplementing transport behavior already maintained by the official SDK. The official `rmcp` 3.1.2 release is Apache-2.0, has an MSRV of Rust 1.88 versus Hydra's pinned 1.96.1, exposes a Tower service compatible with the existing Axum/HTTP 1 stack, carries Axum extensions into tool `RequestContext`, supports MCP `2025-11-25`, and lets a server narrow its negotiated versions.

Decision: pin `rmcp` exactly to 3.1.2 with default features disabled and only `server` plus `transport-streamable-http-server` enabled. Hydra will implement generic identity, binding, authorization, capability, and business dispatch in Fabric; `rmcp` owns the MCP wire protocol and Streamable HTTP mechanics. Hydra's handler will advertise only `ProtocolVersion::V_2025_11_25`. Auth middleware will validate the bearer token and binding before the SDK receives a request, insert `PrincipalContext` into request extensions, and tool handlers will fail closed if that context is absent. SDK OAuth client features are not enabled because Hydra is a resource server and validates access tokens through its own Fabric authority.

Alternatives: continue extending the custom MCP implementation (rejected: duplicates protocol negotiation, GET/POST/SSE, cancellation, and transport security); use a git revision or prerelease (rejected: weaker reproducibility); enable all default/client/auth features (rejected: unnecessary dependency and authority surface); implement Nexus concepts below Fabric (rejected: violates bounded-context and layer rules).

Consequences: MCP transport behavior follows a maintained official implementation while Hydra retains sole control of authentication, tenant binding, schemas, and governed capability dispatch. The dependency graph, audit, and license gates must pass before M1 completes. Future SDK upgrades require an explicit compatibility review and may not silently advance Hydra beyond the `2025-11-25` contract.

### ADR-0022 Asymmetric Nexus token validation and external binding persistence
Context: EP-012 M2 must replace fixed-secret development JWT behavior with an OAuth/OIDC resource-server boundary that verifies asymmetric signatures, standard temporal and issuer/audience claims, principal type, scopes, and a Hydra-owned business binding before any tenant authority exists. Hydra also needs deterministic offline tests and a private-deployment mode that does not require a live identity-provider call. The current Store schema has local users and sessions but no generic external tenant/business binding.

Decision: pin `jsonwebtoken` exactly to 9.3.1 with default features disabled and only PEM support enabled; that release uses the already-supported `ring` backend on native targets. The initially evaluated 11.0.0 `rust_crypto` backend is prohibited because it pulls vulnerable `rsa` 0.9.10 (`RUSTSEC-2023-0071`) with no fixed release, while its AWS-LC backend cannot build reproducibly on Hydra's current Windows toolchain without adding NASM and native environment setup. No advisory ignore or machine-wide tool install is permitted for this seam. Fabric validates only explicitly configured asymmetric algorithms, supports a pinned public-key file or cached JWKS, and never stores bearer tokens in `PrincipalContext`. Add an append-only migration for `external_tenant_binding` with a unique provider/external-tenant/external-business tuple and soft lifecycle states `active`, `disabled`, and `revoked`; expose no delete API. Cryptographic validation precedes binding resolution, and the active binding is the only source of the Hydra tenant for an external principal.

Alternatives: retain the existing HMAC token helper for Nexus (rejected: a shared signing secret and fixed development tenant do not meet the resource-server contract); hand-roll JWT/JWK parsing and signature verification (rejected: unnecessary cryptographic risk); call Nexus synchronously for every request (rejected: avoidable availability and latency coupling); let a header or claim carry the Hydra tenant directly (rejected: caller-selected tenant authority); hard-delete disabled bindings (rejected: loses revocation history and conflicts with Hydra's soft-delete-only posture).

Consequences: Nexus-connected mode gains fail-closed asymmetric identity validation and explicit one-business-to-one-Hydra-tenant resolution while standalone Hydra remains valid with no bindings. Key material may be refreshed and cached without changing binding authority. The selected release avoids both the unfixed RustCrypto RSA advisory and an additional native build prerequisite, but future JWT-library upgrades require another explicit crypto-backend and audit review. The new dependency and migration require `cargo audit`, `cargo deny check`, checked SQLx metadata, Store integration tests, and deterministic issuer fixtures before EP-012 can complete.

### ADR-0023 Acknowledged JetStream delivery with Store-owned outbox leases
Context: EP-014 must replace the coarse core-NATS relay that held a database transaction across network publication, used a tenant-bearing subject, and marked rows published after only a client flush. Hydra already pins `async-nats` 0.49.1 with default features disabled, but its JetStream API was not compiled. Only Store may execute SQL, retries must preserve one logical event ID, and a process crash after broker persistence but before the database receipt must remain safe.

Decision: enable only the existing `async-nats` `jetstream` feature in addition to `ring`; do not add another messaging client. Kernel boot creates or verifies one `HYDRA_CRM_EVENTS_V1` limits-retention stream for `hydra.crm.>` with file storage, one replica, a 30-day age limit, one-million-message limit, 10 GiB byte limit, 1 MiB message limit, and 24-hour duplicate window. Store claims pending rows with short renewable-by-expiry leases and `FOR UPDATE SKIP LOCKED`, then releases the database transaction before network I/O. Kernel publishes the persisted canonical document with its `event_id` as `Nats-Msg-Id`, awaits the server `PublishAck`, and only then asks Store to persist `published_at` plus the positive stream sequence. Transient publish failures release the lease for retry; invalid canonical rows are parked with a fixed redacted reason and remain in Postgres.

Alternatives: retain core NATS plus `flush` (rejected: no stream persistence acknowledgement); hold row locks while awaiting the broker (rejected: couples database concurrency to network latency); mark before publication (rejected: loses events on failure); generate a new ID per attempt (rejected: defeats broker and consumer deduplication); publish tenant IDs in subjects (rejected: leaks authority identifiers and creates unstable routing); use NATS as source of truth (rejected: Postgres audit/outbox remains authoritative).

Consequences: delivery is at-least-once across the Postgres-to-JetStream crash boundary, with producer deduplication inside the configured window and stable event IDs for consumer deduplication beyond it. A duplicate acknowledgement is valid and records the original stream sequence. Startup fails closed if an existing stream's contract fields differ. The active feature graph and checked Store queries must pass dependency/license/security gates and SQLx metadata verification before EP-014 completes.

### ADR-0024 Durable W3C trace carriers and Nexus event readiness
Context: EP-014 must propagate W3C trace context across authenticated Fabric requests, asynchronous governed execution, Store commits, and JetStream publication without turning transient tracing metadata into business authority. Correlation and causation IDs are durable business provenance, while `traceparent` and `tracestate` are operational carriers. Executor work can outlive the initiating HTTP span, and Nexus-connected readiness must not report healthy when the required canonical stream or relay is unavailable. The original EP-014 file list reserved one event migration but did not account for the separate envelope trace carrier needed to restore context after asynchronous dispatch.

Decision: define a provider-neutral `TraceContext` in Store, validate canonical W3C version `00` trace/span identifiers plus bounded `tracestate`, and never accept or persist baggage. Fabric creates a server child for a valid inbound parent and starts a fresh trace for missing or invalid input; authenticated `PrincipalContext` carries that context in-memory with Serde skip semantics. Add additive migration `0013_trace_context.sql` so envelopes and transition audit rows retain the operational carrier separately from `InvocationContext`; outbox already has its own trace column from migration `0012`. Store exposes trace-aware variants behind backward-compatible wrappers, Executor restores and derives child contexts, entity/transition events retain the same trace ID, and Kernel emits only `traceparent`/`tracestate` NATS headers. Use the existing dependency graph instead of adding OpenTelemetry before Hydra has a collector/export pipeline. In Nexus-connected mode, `/readyz` and `/v1/nexus/events/status` require the exact configured JetStream contract and a running relay with a successful iteration; standalone readiness does not require Nexus event infrastructure.

Alternatives: store trace fields inside `InvocationContext` or canonical event payloads (rejected: conflates transient operations with durable business provenance); reuse correlation IDs as trace IDs (rejected: different semantics and validation); accept W3C baggage (rejected: unnecessary PII/secret propagation risk); add a full OpenTelemetry dependency/export stack in EP-014 (rejected: no configured collector and a larger unreviewed graph); treat core NATS connectivity as event readiness (rejected: does not prove stream contract or relay operation).

Consequences: old envelopes and outbox rows remain readable with null trace context; external paths preserve one trace ID across asynchronous child spans without serializing access tokens, prompts, customer data, or baggage. Invalid incoming trace metadata cannot alter durable correlation and starts a safe local trace. Required event outages now fail Nexus-connected readiness closed, while Hydra remains independently deployable. Migration `0013` and the trace-aware approval/idempotency/executor files are justified additions to EP-014's expected change set and require refreshed SQLx metadata plus full Store/Fabric/Kernel regression tests.

### ADR-0025 Fake Nexus consumer stream utility
Context: EP-014 M5 must exercise the pinned `async-nats` durable pull-consumer API. Its message iterator implements the standard futures `Stream` trait and intentionally relies on `futures_util::StreamExt` for bounded `next()` consumption. `futures-util` 0.3.32 is already present in Hydra's lockfile through the accepted runtime graph, but Kernel did not directly declare it.

Decision: declare the already-locked `futures-util` 0.3.32 at workspace level and use it only as a `hydra-kernel` dev-dependency for the fake Nexus consumer fixture. Do not add it to production Kernel dependencies or introduce another NATS abstraction.

Alternatives: hand-poll the third-party stream type (rejected: brittle and needlessly bypasses the documented client API); add a second async-stream crate (rejected: larger graph); skip the real consumer and mock delivery (rejected: cannot prove durable resume and acknowledgement semantics).

Consequences: production artifacts gain no new direct runtime dependency, the lockfile should remain on the existing version, and M5 can use the official bounded pull API. Dependency/license/security gates still run before EP-014 completion.

### ADR-0026 Fake Nexus HTTP and JWT harness dependencies
Context: EP-015 must prove the complete resource-server boundary through real loopback HTTP, including a locally served JWKS document, signed service/agent/human tokens, MCP Streamable HTTP calls, REST calls, and cross-business denial. Kernel's production graph already receives `reqwest` and `jsonwebtoken` transitively through Fabric, but its integration-test crate cannot use transitive crates without direct declarations.

Decision: declare the existing workspace-pinned `reqwest` 0.12.28 and `jsonwebtoken` 9.3.1 only as `hydra-kernel` dev-dependencies. Reuse the same deterministic Ed25519 fixture already accepted by Fabric auth tests, serve only its public JWK from a loopback Axum issuer, and keep the private DER bytes confined to test source. Build the E2E application from the same public Store, persisted Governor, RuntimeServices, Fabric AppState, Executor, relay, and JetStream publisher types used by Kernel production wiring.

Alternatives: call service methods directly (rejected: would not prove HTTP auth, Origin, protocol, or tenant binding); hand-write HTTP/JWT codecs (rejected: unnecessary parser and cryptographic risk); add a live Nexus or cloud IdP dependency (rejected: nondeterministic and violates the repository boundary); add either crate to Kernel production dependencies (rejected: the harness is test-only).

Consequences: the production artifact and resolved lock graph do not gain new packages or authority. E2E tests can exercise the real external boundary with no cloud credentials, while dependency, license, secret-scan, and full verification gates still apply before EP-015 completion.

### ADR-0027 Pinned age vault and runtime SecretSource boundary
Context: Hydra's documented bridge boundary requires named credentials to remain outside Git, image layers, SQL, events, and logs, while the kernel must fail closed in staging and production when those credentials are unavailable. The repository had only `StaticSecretSource` despite documenting an age-encrypted file vault. EP-018 needs a persisted format and owner tooling without introducing a second secret authority or exposing values through the adapter ABI.

Decision: pin the direct workspace dependency `age = "=0.11.1"` and use its standard scrypt passphrase-encrypted file format for a versioned, bounded JSON document. Load it only through `bridge_host::VaultSecretSource`; keep adapter access constrained by the existing per-adapter named grants; provide `hydra-vault` as a thin owner tool that reads values from stdin, lists names/status only, and rotates by decrypting with the old key before atomically replacing the file with the next key. Use the already locked `windows-sys = "=0.61.2"` only on Windows to call the platform atomic replacement API; Linux uses same-directory rename with owner-only file permissions. The dependency's published license is MIT OR Apache-2.0, and `cargo deny check` passes advisories, bans, licenses, and sources with the existing repository warnings.

Alternatives: keep the documented vault as an unimplemented placeholder (rejected: production startup would not have a real secret source); invent custom encryption (rejected: unnecessary cryptographic risk and interoperability loss); store credentials in Postgres or environment variables (rejected: violates the named-secret and backup boundary); add a general secret-management service dependency (rejected: no authorized external service or credential and would couple standalone Hydra); remove and rename the existing vault on Windows (rejected: a failed replacement could destroy the last good encrypted artifact).

Consequences: the kernel now constructs a loaded read-only source in configured staging/production mode and disables only bridge secrets for an absent development vault. Invalid, unreadable, missing, or undecryptable staging/production vaults fail before serving. The encrypted artifact remains owner-operated and its restore/custody drill is still an EP-010 gap; bridge lifecycle execution remains unavailable and is not made available by loading secrets. Future vault format changes require a new document version and an explicit ADR.

### ADR-0028 Optional Nexus model, A2A, and signed skill boundaries
Context: EP-011 through EP-015 established the required authenticated Nexus CRM seam, while EP-016 was intentionally deferred. The current A2A specification has a released 1.0.0 protocol with JSON-RPC PascalCase methods and a durable task model. The upstream Agent Skills specification defines `SKILL.md` packaging and progressive disclosure but does not define a signature or execution trust protocol. Hydra must add these optional features without creating an agent authority path, bypassing TOKENKILLER, or making Nexus a runtime dependency.

Decision: implement an optional OpenAI-compatible `nexus` LLM provider inside `llm-router`, sourced through TOKENKILLER and the named-secret vault; add provider/privacy/budget provenance to the existing TK response contract; implement a durable tenant-scoped A2A 1.0 JSON-RPC facade with only `SendMessage`, `GetTask`, `ListTasks`, and `CancelTask`; and define a Hydra-local `hydra-skill.json` Ed25519 JWS manifest over the upstream `SKILL.md` content hash. Reuse the already locked `serde_yaml` 0.9.34, `jsonwebtoken` 9.3.1, and `sha2` 0.10.9 graph for metadata and signature verification rather than adding a package manager or a second crypto stack. Unsupported A2A streaming/push and unimplemented bridge workflows remain explicitly unavailable. Skill discovery is declarative and never executes package scripts or grants credentials/tools.

Alternatives: add a full A2A SDK or an unreviewed skill package manager (rejected: no dependency/license fit has been proven and the required surface is bounded); invent a second CRM/workflow store (rejected: Store remains the only SQL boundary and tasks are an additive persistence record); let skills carry HMAC secrets or tool lists (rejected: violates least authority and named-secret policy); call a Nexus model from agents directly (rejected: violates TK-1 through TK-6).

Consequences: Hydra gains optional protocol-compatible discovery and durable workflow status while standalone mode remains unchanged. The gateway token is referenced by vault name, and skill public trust anchors are owner-controlled. A2A task persistence requires additive migration `0014_a2a_tasks.sql` and refreshed SQLx metadata. Production readiness remains partial until staging protocol drills, trust-anchor custody, restore/rollback, and human sign-off are evidenced.

### ADR-0029 Governed prebuilt bridge lifecycle boundary
Context: EP-019 closes the code-owned gap between the existing Wasmtime/WIT
BridgeHost and the governed execution path. Fabric previously represented
bridge registration with an envelope but pause/resume could write adapter KV
directly, while the kernel did not register a lifecycle handler. A safe first
slice must not become a second CRM abstraction or pretend that discovery,
synthesis, sync, canary, or promotion are implemented.

Decision: persist tenant-scoped adapter identity, component reference, digest,
grant projection, descriptor, lifecycle state, revision, and transition
history in Store. Resolve only prebuilt components below `HYDRA_ADAPTERS_PATH`,
probe them through the existing BridgeHost with named grants and fuel, and
register typed `bridges/deploy_adapter`, `bridges/pause_adapter`, and
`bridges/resume_adapter` handlers only when the component root and configured
SecretSource are available. Fabric remains proposal-only for external
requests and projects status from the registry; unsupported lifecycle
capabilities remain unavailable.

Alternatives: retain direct adapter KV mutation (rejected: bypasses durable
state and governed execution); accept caller paths or raw credentials
(rejected: violates the BridgeHost grant boundary); implement synthesis or
sync in this seam (rejected: no accepted contract or runtime proof exists).

Consequences: prebuilt bridge activation is now executable in a configured
runtime while standalone Hydra remains valid and missing configuration fails
closed. The registry is integration metadata and never a CRM source of truth.
EP-010 remains partial until staging, recovery, operational, and human-owned
evidence is available.

### ADR-0030 Operational readiness and recovery helper boundary
Context: EP-019 established local bridge lifecycle and Nexus E2E behavior, but the kernel readiness response was only a short first-failure string and the database helpers could publish an unvalidated archive or drop a fixed restore-check database without an explicit disposable-target acknowledgement. These are code-owned risks even though the D1-D5 staging drills remain operator-owned.

Decision: preserve the `/healthz` and `/readyz` bodies for existing consumers, add `/readyz/details` with deterministic non-secret checks, and make configured bridge lifecycle availability part of readiness. Make backups private, temporary-file based, archive-validated, and atomically renamed. Make restore verification require `HYDRA_RESTORE_CONFIRM=ephemeral`, refuse production environment markers, generate a unique target name internally, use the `postgres` maintenance database, restore with `--exit-on-error --single-transaction`, and remove only that generated target. Cover the shell behavior with fake PostgreSQL clients in the mandatory local verifier.

Alternatives: replace `/readyz` with JSON (rejected: breaks existing probes); leave helper safety to operators (rejected: the repository would continue to provide an unsafe default); execute the regression suite against the shared local database (rejected: violates the test-database safety boundary); claim D1 evidence from the helper test (rejected: a fake-client test cannot prove a staging restore).

Consequences: local readiness diagnostics and helper safety are stronger and repeatable without adding dependencies or migrations. EP-010 remains partial until real staging restore/rollback, soak, recovery, security, privacy, performance, accessibility, observability, and human sign-off evidence exists.

### ADR-0031 Release provenance and truthful nightly gate boundary
Context: EP-010 still lacked a repository-defined SBOM/provenance policy, while the tag workflow pushed images without a signed digest attestation. The nightly workflow could also report success after an ignored Cargo test command discovered no tests. GitHub's current attestation action accepts a fully qualified image name plus the Buildx digest; its documented `artifact-metadata` permission is for the optional organization-owned storage-record path, which this personal repository must disable.

Decision: enable `provenance: mode=max` and `sbom: true` on the release Buildx step, capture its digest, and invoke `actions/attest@v4` with `push-to-registry: true` under a least-privilege image-job permission block. Because the current repository is private and user-owned, set `create-storage-record: false` and omit `artifact-metadata: write`, which is only needed for the organization-owned storage-record path. Add a local static policy checker and make missing attestation configuration fail closed. Replace the nightly raw ignored-test command with a wrapper that preserves Cargo status and requires a positive test count, the named `c9_soak_10k` test, and an `ok` result. Use a separate finite test-only fuel grant for the 10k fixture; do not change production BridgeHost fuel policy.

Alternatives: retain default BuildKit metadata (rejected: not a reviewed signed release artifact); use the legacy `attest-build-provenance` wrapper (rejected: the current official action recommends `actions/attest`); allow a warning-only or `continue-on-error` path (rejected: false-green release/nightly gates); remove or shrink the 10k soak (rejected: would weaken the required workload evidence).

Consequences: the repository now expresses and statically validates the intended release boundary, and nightly local execution cannot pass with an empty ignored test set. A real GitHub tag run, registry acceptance, attestation verification, staging release, and human production-readiness evidence remain external and unexecuted. EP-010 remains partial.

### ADR-0032 Explicit outbound proxy boundary
Context: the Compose topology already isolates Tinyproxy as the intended external network path, but direct `reqwest` clients in the LLM, OIDC, Fabric, and BridgeHost seams previously relied on ambient proxy environment behavior. That is not a typed application contract and could allow staging or production startup with an unverified outbound path.

Decision: add `HYDRA_EGRESS_PROXY_URL` to Kernel configuration, require an absolute HTTP(S) value without embedded credentials in staging and production, and pass it explicitly to configured external HTTP clients. Preserve unconfigured constructors only for standalone development and deterministic tests. Make a static policy checker mandatory in preflight and the full verifier.

Alternatives: rely on `HTTP_PROXY`/`HTTPS_PROXY` alone (rejected: ambient behavior is not fail-closed configuration); route database or NATS traffic through the proxy (rejected: unrelated internal boundaries); redesign Tinyproxy allowlists in this plan (rejected: destination policy is a separate operator-controlled concern); or remove test constructors (rejected: would break deterministic offline validation and standalone operation).

Consequences: missing or malformed proxy configuration fails before staging/production runtime startup, and proxy URLs are not echoed in errors. Local tests prove construction and policy wiring only; real staging DNS/TLS, ACL, IdP, and external-provider evidence remains EP-010 work.

### ADR-0033 Store-owned tenant export and non-destructive retention preview
Context: EP-010 requires privacy/data evidence, but Hydra does not yet have an owner-approved retention duration, purge scheduler, or staging export demonstration. The existing Store already owns canonical tenant data, soft-delete state, append-only event history, outbox records, and the TOKENKILLER ledger. A read-only projection is the smallest code-owned seam that improves operator visibility without creating a second CRM source of truth or authorizing destructive cleanup.

Decision: implement `TenantDataRepo` in Store with a versioned, bounded tenant export and an age-based retention preview. The export includes canonical entities, same-tenant relationship edges, and safe event metadata while retaining soft-deleted records. The preview reports counts and oldest/newest timestamps for soft-deleted entities and aged operational records, plus pending outbox count; it never deletes, marks, schedules, or publishes anything. Expose both through authenticated local REST routes requiring `Admin`, with tenant authority derived from the verified `AuthCtx` session. Do not accept a tenant ID from a query, path, body, or header as authority, and do not select a legal retention period in code.

Alternatives: add a purge scheduler now (rejected: no owner-approved legal policy or staging safety evidence); export raw outbox/TOKENKILLER payloads (rejected: secrets, prompts, and provider data do not belong in a customer export); put SQL in Fabric (rejected: violates the six-layer law); or expose a Nexus mutation endpoint (rejected: this plan is a local read-only privacy/data seam and Nexus mutations remain governed ActionEnvelope proposals).

Consequences: operators can inspect a deterministic tenant projection and estimate retention candidates locally, while soft-delete-only semantics and Postgres authority remain intact. The JSON contract is `hydra.tenant-data.v1` and is bounded at 10,000 records per export. Production readiness remains blocked on owner policy, purge/scheduling design, staging privacy/restore evidence, and human sign-off. No migration or new dependency is required; checked SQLx metadata is refreshed for the Store queries.

### ADR-0034 Retire the migration-owned development seed
Context: `migrations/0007_auth.sql` creates a fixed `admin` account in every database, and Shell form login reaches `SessionStore` without an environment guard. Existing sessions for that row could also remain usable. The historical Argon2 hash does not validate the documented `hydra-dev` password, so retaining a development exception would be both unsafe and non-functional.

Decision: add migration `0016_auth_seed_hardening.sql` with additive `auth_source` and `disabled_at` fields, mark only the known migration-owned row as `development_seed` and disabled, and make `SessionStore::authenticate` and `SessionStore::lookup` reject that source in every environment. Preserve the separate `HYDRA_ENV=dev` bearer fixture as a local API test path; it does not create or re-enable a database user. Keep owner-controlled active-user/bootstrap provisioning outside this plan.

Alternatives: delete the historical row (rejected: erases audit history); accept the seed only in development (rejected: fixed credential remains a dangerous path and its hash does not match the documented password); add an owner endpoint in this patch (rejected: credential custody, binding, audit, and recovery need a separate reviewed contract).

Consequences: the repository has no form-login or session-lookup path for the fixed seed, while active operator credentials retain their existing role and tenant behavior. The migration is additive and includes an idempotent constraint block plus a revert note. Production/staging owner bootstrap and identity evidence remain EP-010 gaps.

### ADR-0035 Recover durable approved execution without replaying uncertain work
Context: the Governor and Store persist an envelope as `Approved`, but the Kernel's ExecuteToken delivery channel was process-local. A restart after approval could strand work. Conversely, an envelope already in `Executing` may have completed an external side effect before a crash, so blind replay could duplicate it.

Decision: add a bounded Store query for tenant-qualified `Approved` identities and have the supervised ExecutorWorker scan immediately and periodically. Recovery enters the same private Executor path and relies on the existing tenant-scoped row lock and legal `Approved -> Executing` transition. Add Store-backed readiness reporting for `Executing` rows older than 15 minutes; fail closed and require operator evidence rather than automatically resetting or replaying them.

Alternatives: persist a second queue in NATS or Redis (rejected: Postgres is already authoritative and a second command source would widen the boundary); expose a public ExecuteToken constructor (rejected: it would mint execution authority outside Governor); reset stale `Executing` to `Approved` (rejected: external outcome is uncertain and duplicate side effects are unsafe).

Consequences: approved work survives a Kernel restart without adding a queue dependency, while ambiguous in-flight work becomes visible and blocks readiness. Local tests prove the seam, but staging crash/recovery, rollback, and operator evidence remain EP-010 gates.

### ADR-0036 Store-backed distributed rate-limit authority
Context: the production Kernel previously used a process-local fixed-window map. That permits each replica to admit its own quota and therefore cannot serve as the sole multi-replica admission boundary. Fabric already derives principal/network keys and Store is Hydra's only SQL boundary.

Decision: add an additive Postgres `rate_limit_window` table and a Store repository that performs an atomic database-time upsert, caps counts at `max_requests + 1`, and prunes old windows opportunistically. Fabric persists only a versioned SHA-256 digest of the derived key. The real Kernel constructs this Store-backed limiter; local synchronous construction remains available only for deterministic tests and fixtures. Store or pruning failure returns a generic 503 and never falls back to a per-process quota.

Alternatives: use Redis or NATS (rejected: introduces a second operational authority and dependency); persist raw principal, tenant, or IP text (rejected: unnecessary disclosure of operational identifiers); silently fall back to local state (rejected: creates an unbounded replica bypass); or make rate admission establish tenant authority (rejected: authentication and authorization remain separate Fabric boundaries).

Consequences: replicas share one bounded Postgres admission decision, exceeded requests retain the existing 429/`Retry-After` contract, and authority outages fail closed. Local tests prove atomicity, key isolation, pruning, digest non-disclosure, and 503 mapping. Multi-replica staging behavior, outage drills, and human production sign-off remain EP-010 evidence.

### ADR-0037 Bounded runtime request metrics in Kernel
Context: the Kernel already exposed a process-local Prometheus text registry,
but its counter and histogram mutation helpers were test-only and no real
request path recorded request totals or latency. EP-010 still requires live
observability evidence, so local instrumentation must be useful without
pretending to provide dashboards or alert delivery.

Decision: enable the existing counter and histogram helpers for runtime use and
install a Kernel L6 middleware that records method, fixed route taxonomy,
status class, and duration. Unknown or user-shaped paths map to `/other`,
query strings are ignored, and no tenant, identity, token, or customer data is
stored in labels. Keep the registry process-local and preserve `/metrics` as
the existing diagnostic surface; live scrape authentication, dashboards,
alerts, and staging drills remain EP-010 work.

Alternatives: record raw paths (rejected: unbounded cardinality and possible
identifier disclosure); add a new telemetry dependency (rejected: outside
this bounded seam); or claim local counters as production observability
evidence (rejected: no live monitoring or operational drill occurred).

Consequences: running Kernel requests now contribute bounded totals and
latency to `/metrics`, while a restart resets the diagnostic registry and
multi-replica aggregation remains an operator-owned deployment concern. No
database, dependency, external endpoint, or production behavior changed.

### ADR-0038 Owner-operated bootstrap without an HTTP authority path
Context: migration 0016_auth_seed_hardening.sql disables the historical
development seed, while Nexus authentication requires an active operator
identity and an explicit external business-to-Hydra tenant binding. Direct SQL
would bypass Store validation and create an undocumented provisioning path.

Decision: add the Rust-only hydra-admin binary and an OperatorRepo in Store.
The binary reads passwords from stdin, uses Fabric's Argon2id helper, requires
HYDRA_ADMIN_CONFIRM=I_UNDERSTAND for mutations, and exposes only operator
create/list/status plus binding create/status operations. User creation is
transactional, disabling deletes only that user's active sessions, and
bindings retain their rows while changing their existing soft status. The
tool never runs migrations and is not reachable through HTTP, MCP, or Nexus.

Alternatives: add a public bootstrap endpoint (rejected: it would widen the
remote authority surface); revive the seed (rejected: fixed credentials are
not acceptable); or issue direct SQL runbooks (rejected: it would violate the
Store-only SQL boundary).

Consequences: an owner has a supported path to provision the identities needed
by the existing control plane, while credential custody, staging identity,
backup/recovery, and human production approval remain explicit operator-owned
gates. No new dependency or migration was required.

### ADR-0039 Supervise Kernel lifecycle and bound dependency operations
Context: the Kernel previously waited only for Ctrl-C, while Docker and most
orchestrators terminate Unix processes with SIGTERM. Relay and executor tasks
were joined only after HTTP serving ended, and readiness could wait
indefinitely on a database, NATS, event-status, or recovery operation. A
process that silently loses a required worker can therefore continue serving
requests without an honest readiness signal.

Decision: use existing Tokio signal, watch, and time facilities to unify
Ctrl-C, SIGTERM, and internally requested shutdown. Keep relay and executor
handles under a Kernel supervisor; an unexpected early exit requests a
coordinated drain and returns a failure, while normal shutdown joins each task
within `HYDRA_SHUTDOWN_TIMEOUT_SECONDS` and aborts only an owned task that
exceeds the bound. Add drop-safe executor health and expose shutdown/worker
state through the existing readiness contract. Bound startup and individual
readiness dependency operations with `HYDRA_DEPENDENCY_TIMEOUT_SECONDS`.

Alternatives: rely on the container's default SIGTERM behavior (rejected:
the process can terminate without draining or marking readiness); detach
background tasks (rejected: failure would be silent); add a second supervisor
dependency (rejected: Tokio already supplies the required primitives); or
replace `/readyz` with a new response contract (rejected: existing probes
must remain compatible).

Consequences: local process behavior is deterministic and fail-closed under
task failure or dependency stalls, with no new dependency, migration, or
database authority. Local tests do not prove an orchestrator's actual signal
delivery or staging drain time; termination-drain, crash-loop, and human
operational evidence remain EP-010 gaps.

## Rules for adding decisions
Any new dependency, schema change, ABI change, autonomy-cell default change, or S0–S2 prompt-segment change requires an ADR entry BEFORE merge.

### ADR-0040 Durable tenant autonomy freeze overlay
Context: the D5 operational procedure named a `hydra cell freeze` command,
but Hydra had no durable freeze state. Replacing autonomy cells directly
would destroy the pre-freeze matrix, would not provide a clear operator
status, and could leave the persisted Governor cache serving stale autonomy.

Decision: add an additive Store-owned `autonomy_freeze` overlay keyed by
Hydra tenant. When frozen, matrix resolution presents every stored cell as
canonical L1 while preserving the stored levels for thaw. Freeze/thaw bumps
the existing tenant autonomy revision and emits one bounded
`autonomy.freeze_changed` event for each actual state change. Expose the
mutation only through the existing confirmation-gated `hydra-admin` binary;
do not add a public HTTP or agent authority path.

Consequences: newly proposed actions become `SuggestOnly` under L1 and
already-dispatched ExecuteTokens are not revoked or replayed. The control is
tenant-safe and cache-safe locally, but staging D5 timing and operator
evidence remain EP-010 requirements.

The freeze transition also uses the typed
`hydra.crm.autonomy.freeze_changed.v1` event and canonical outbox path so
Nexus consumers observe the operational boundary without direct database
access.

### ADR-0041 Optional internal observability profile

The repository's Kernel metrics and Prometheus alert rules are wired through
an explicit, profile-gated Prometheus/Alertmanager pair. Prometheus may reach
only the Kernel metrics endpoint over an internal ingress network, and
Alertmanager is isolated on a separate internal observability network. The
checked-in receiver has no outbound destination; staging and production must
provide an operator-reviewed notification override. This keeps standalone
Hydra unchanged, avoids inventing a secret or external endpoint, and makes
local rule evaluation executable without overstating EP-010 evidence.

### ADR-0042 Optional scheduled Postgres backup profile

The existing atomic `db-backup.sh` helper is invoked by an explicit `backup`
Compose profile using `postgres:16-alpine` client tools. The service has only
`data-internal` access, writes to a named local volume, validates a minimum
interval, and exits on helper failure. It never prunes archives, performs a
restore, or copies data off host. This closes the local scheduling seam while
leaving retention, off-box replication, JetStream/vault recovery, and staging
evidence under operator control.

### ADR-0043 Bounded TOKENKILLER bridge-mapping synthesis

Context: EP-019 intentionally left BridgeEngineer synthesis unavailable because
there was no accepted contract for model-mediated bridge work. Hydra already
has a bounded `MappingYaml` TOKENKILLER contract, a provider-neutral router,
and durable authenticated A2A tasks. The missing seam is a reviewable mapping
proposal, not a reason to generate or execute adapter code.

Decision: add the existing `tokenkiller` path dependency to `agents` and route
BridgeEngineer mapping proposals through `Session::complete("bridge_mapping")`
with fixed S0-S2 segments, bounded metadata, NukeGuard/repair/ledger behavior,
strict post-contract validation, deterministic YAML normalization, and redacted
provider provenance. Kernel constructs the optional runtime over its existing
router and Store ledger sink; Fabric exposes it only through authenticated,
tenant-scoped, idempotent A2A proposal tasks. Wasmtime activation, sync,
conformance, canary, promotion, ActionEnvelope approval, and CRM mutation are
explicitly outside this seam.

Alternatives: use the historical `bridge_codegen` route (rejected: it would
create executable output before review); call `llm-router` from agents (rejected:
violates TK-1 through TK-6 and the layer law); add a synthesis table (rejected:
existing A2A task persistence is sufficient); or report availability without a
runtime handler (rejected: capability truth must reflect configured support).

Consequences: configured runtimes report Experimental mapping synthesis and
standalone runtimes remain disabled without changing existing behavior. The
proposal artifact is non-executable and A2A failures become bounded durable
`failed` tasks. EP-019's activation boundary and EP-010 production-readiness
evidence remain open.

### ADR-0044 Tenant-scoped bridge scratch state

Context: EP-019 made adapter identity tenant-scoped, but the historical
`adapter_kv` table used only `(adapter_id, k)`. Two tenants using the same
adapter ID could therefore share scratch state if the old Store path remained
active.

Decision: add `tenant_adapter_kv` with primary key
`(tenant_id, adapter_id, k)`, require the governed envelope tenant in
BridgeHost lifecycle probes, and keep the old unscoped Store methods only as
fail-closed compatibility shims. Do not assign tenants to historical rows;
they contain no authoritative tenant identity.

Alternatives: keep the global table (rejected: violates tenant isolation),
delete or rewrite historical rows (rejected: destructive and unauthorized),
or guess ownership from adapter configuration (rejected: configuration is not
tenant authority).

Consequences: equal adapter IDs are isolated in new runtime paths and old
callers receive a deterministic invariant error. Synchronization and legacy
row migration remain unavailable until a separately governed plan establishes
an authoritative migration policy.

### ADR-0045 Governed manual bridge synchronization

Context: EP-019 intentionally stopped at governed prebuilt activation, pause,
and resume, while the existing WIT ABI already exposed an incremental
`changes-since` feed. Applying that feed needs durable cursor ownership,
conflict handling, and a tenant-safe external request path without creating a
second CRM abstraction.

Decision: add one manually invoked `hydra.bridges.sync` capability. MCP and
`POST /v1/nexus/bridges/{id}/sync` create the same authenticated,
idempotent ActionEnvelope. The Kernel invokes `changes-since` only through the
existing BridgeHost grants and typed handler. Store advances a
tenant/adapter/kind cursor only in the same transaction as canonical
bridge-origin changes and outbox records; invalid pages park bounded conflict
metadata and leave the cursor unchanged.

Alternatives: accept a caller cursor (rejected: it could skip or cross tenant
history), write provider payloads into a new CRM table (rejected: Hydra CDM is
canonical), or add a background scheduler (deferred: it would introduce an
unvalidated worker lifecycle and autonomy policy).

Consequences: manual incremental synchronization is locally executable when a
configured adapter advertises the capability. Full relist, scheduling,
synthesis, conformance, canary, and promotion remain unavailable. EP-010
production readiness is unchanged and remains partial.

### ADR-0046 Read-only governed bridge conformance

Context: EP-035 provides a governed manual synchronization path, while the
existing A2A catalog still listed `bridge-conformance` without a typed runtime.
Conformance must inspect a configured adapter without creating a second bridge
abstraction or turning an audit read into an implicit CRM mutation.

Decision: implement `bridge-conformance` as an authenticated A2A workflow that
requires `hydra.bridges.read`, resolves the tenant-scoped active adapter and
its persisted digest/grant/configuration through Store, and invokes only the
bounded read-side BridgeHost/WIT exports. Return digest, descriptor metadata,
counts, fuel, and a deterministic report; never return raw records, secrets,
provider bodies, or mutation results. A2A task state and idempotency remain the
only durable workflow records; no CRM event or outbox row is created.

Alternatives: expose component paths or grants from Nexus (rejected: caller
authority and sandbox escape risk); reuse synchronization (rejected: it writes
canonical CRM state); or leave the catalog entry advertised as unavailable
(rejected: capability truth requires the configured runtime to be executable).

Consequences: configured adapters can be validated through the existing
authenticated A2A seam, while missing/inactive/digest-invalid/malformed or
cross-tenant requests fail closed. Activation, synchronization, canary,
promotion, generated code, and EP-010 production readiness remain separate
gates. No production deployment occurred.

### ADR-0047 Governed bounded full-relist fallback

Context: the normative WIT bridge contract states that an adapter without
`incremental-sync` falls back to full-relist diffing, but EP-035's handler
rejected those adapters and Store only applied incremental pages. Leaving the
fallback unavailable made descriptor capability truth and runtime behavior
diverge.

Decision: extend the existing manually invoked `hydra.bridges.sync` handler to
select incremental or full-relist mode from the persisted descriptor. BridgeHost
follows only bounded WIT `list` pages and rejects repeated cursors, duplicate
identities, invalid records, and fixed page/record/byte overages. Store applies
the complete validated snapshot in one tenant-scoped transaction, avoids
version churn for unchanged active rows, revives matching tombstones, and
soft-deletes only missing active bridge-origin rows. Receipts contain strategy,
counts, page count, and resource metadata, never raw records.

Alternatives: keep rejecting non-incremental adapters (rejected: violates the
WIT contract); apply pages independently (rejected: partial snapshots could
soft-delete valid records); add a scheduler (deferred: worker lifecycle and
autonomy policy are not yet validated).

Consequences: manual full-relist synchronization is now code-owned and locally
verified without a migration or new CRM abstraction. The capability remains
Governor-gated and tenant-scoped. Scheduling, mapping activation, canary,
promotion, staging, and EP-010 production readiness remain separate gates; no
production deployment occurred.

### ADR-0048 Hermetic real-child smoke database

Context: the real Kernel smoke test previously forwarded the root
`DATABASE_URL`, so its health/readiness result depended on hidden root-schema
migration state. The existing Store `TestDb` already creates a unique schema
and applies the embedded migrations, but SQLx 0.8.6's `PgConnectOptions::to_url_lossy`
does not serialize startup `options`.

Decision: expose `TestDb::scoped_database_url()` as a test-only child-process
boundary. It validates the caller URL, appends a percent-encoded Postgres
`options=-c search_path=<unique_schema>,public` parameter, and never logs the
result. The smoke harness owns one outcome-safe lifecycle: create/migrate the
schema, start the real Kernel child, preserve the existing endpoint assertions,
stop the child, and attempt schema cleanup even when setup or assertions fail.

Alternatives: pre-migrate the root database (rejected: hidden shared state),
change health/readiness or disable the rate limiter (rejected: weakens the
production path), or add a URL dependency solely for the test helper
(rejected: the bounded encoded query is sufficient and introduces no new
crate).

Consequences: the smoke test passed twice against a fresh empty loopback
database, with no root migrations applied manually; the disposable cluster
was stopped afterward. This proves local hermeticity only and does not satisfy
EP-010 staging, recovery, soak, or human-readiness evidence.

### ADR-0049 Owner-controlled governed bridge scheduling

Context: EP-037 makes bounded full-relist synchronization executable through
the existing governed `hydra.bridges.sync` command, but production operations
still need a durable cadence without adding a second CRM mutation path.

Decision: add an additive tenant-scoped schedule table with bounded interval
and page limits, soft-disable operations, expiring `SKIP LOCKED` leases, and a
deterministic slot idempotency key. An explicitly enabled, supervised Kernel
worker calls a fixed Fabric internal proposal method that uses actor
`hydra-scheduler`, origin `hydra.scheduler`, and the existing Governor,
Executor, audit, and outbox path. Owner mutations remain local and require
`HYDRA_ADMIN_CONFIRM=I_UNDERSTAND`; scheduling is disabled by default.

Alternatives: call BridgeHost from a timer (rejected: bypasses envelopes and
Governor), expose a generic scheduled command endpoint (rejected: adds caller
authority and a second mutation surface), or retry failed provider work in the
worker (rejected: approval and retry semantics belong to the existing governed
execution path).

Consequences: schedule claims are replica-safe and stale leases are
reclaimable; an equivalent slot retry returns its existing envelope and a
conflicting reuse fails deterministically. Local disposable tests prove the
claim-to-envelope path. EP-010 staging, multi-replica, provider, recovery,
soak, and human sign-off evidence remains outstanding; no production
deployment occurred.

### ADR-0050 Scheduler concurrency evidence and bounded metrics

Context: EP-039 provides a durable `SKIP LOCKED` scheduler lease, but its
existing local evidence did not exercise two worker-level polls or a proposal
created before lease completion. The scheduler is a library component while
the historical metrics module was binary-private.

Decision: share the existing dependency-free Kernel metrics registry with the
library scheduler, record only fixed scheduler outcome labels, and add
disposable tests for two-worker exclusivity, interrupted-proposal replay, and
stale lease completion. Correct the validation command to target the library
metrics tests so a binary filter cannot silently discover zero tests.

Alternatives: add a durable metrics store (rejected: external operational
authority), put tenant IDs in labels (rejected: cardinality and disclosure),
or expose a second library registry (rejected: `/metrics` would omit worker
outcomes).

Consequences: local diagnostics now show scheduler stages and the worker-level
lease/idempotency contract is executable. Metrics reset on restart and remain
non-authoritative; EP-010 multi-replica staging, live monitoring, recovery,
soak, and human sign-off evidence remain open.

### ADR-0051 Truthful production-readiness evidence parsing

Context: the EP-010 readiness script already required recent PASS rows for
D1-D5, but its launch-table check only required non-empty matching text. A
ledger with PENDING or BLOCKED launch results could therefore become
false-green once the drill rows existed.

Decision: use one dependency-free POSIX-shell evidence library for exact drill
and launch-row parsing. Require explicit PASS status, named owner/operator,
non-placeholder evidence, exactly one row per required check, and non-future
evidence no older than 30 UTC days. Keep the checked-in ledger pending and add
fixture tests for both valid and invalid evidence.

Alternatives: retain the loose grep (rejected: false-green), accept any
status with a date (rejected: status is the gate), or add a runtime/database
evidence store (rejected: this is an operator-owned release ledger and no
staging authority exists in the repository).

Consequences: the local production-readiness command is stricter and cannot
claim launch readiness from placeholder rows. EP-010 remains partial until
real staging drills, reviews, soak, and human sign-off are performed. No
staging or production action occurred.

### ADR-0052 Keep Kernel metrics internal to the ingress topology

Context: EP-027/031/040 added a bounded process-local `/metrics` endpoint and
an internal Prometheus profile. The reference Caddyfile used a catch-all
`reverse_proxy kernel:8080`, which also forwarded `/metrics` from the public
HTTPS listener even though Prometheus already had a direct internal target.

Decision: add a named `/metrics*` matcher and dedicated `404` Caddy handle
before the catch-all proxy. Keep Kernel's direct route and Prometheus's
`ingress-internal` scrape unchanged, and enforce the structure with a static
policy plus negative fixture tests.

Alternatives: expose metrics through public auth (rejected: no requirement
for public metrics and it expands the trust boundary), remove the metrics
route (rejected: breaks internal observability), or rely on network placement
alone (rejected: Caddy is intentionally public and the route was reachable).

Consequences: public `/metrics*` requests fail closed without operational
disclosure, while internal Prometheus retains its scrape path. Local policy
validation does not claim staged Caddy startup, live monitoring, or human
observability review; EP-010 remains partial.

### ADR-0053 Make public smoke validation respect internal metrics

Context: EP-042 correctly denied `/metrics*` at Caddy, but the public branch of
`scripts/smoke-test.sh` still requested `$HYDRA_SMOKE_URL/metrics`. A real
ingress smoke would therefore fail for the correct security behavior.

Decision: retain `HYDRA_SMOKE_URL` for public health/readiness, skip public
metrics explicitly, and add optional `HYDRA_SMOKE_INTERNAL_METRICS_URL` for a
separately reachable internal metrics endpoint. Reject equal URLs and keep
the direct local Kernel smoke metrics assertions unchanged.

Alternatives: reopen public metrics (rejected: weakens EP-042), remove all
metrics smoke coverage (rejected: loses the internal contract), or infer an
internal route from the public URL (rejected: unsafe and environment-specific).

Consequences: public smoke validation no longer conflicts with the ingress
security boundary. Operators must provide an approved internal URL to validate
metrics in an external smoke; no staging or production evidence is implied.

### ADR-0055 Wire configured bridge egress through the explicit proxy

Context: EP-022 defined and tested `ReqwestEgressClient::new_with_proxy`, but
the real Kernel `build_bridge_lifecycle` path still injected
`DenyEgressClient`. A configured adapter could therefore be reported active
while every granted HTTP call was denied.

Decision: construct the existing `ReqwestEgressClient` from the validated
`LlmRuntimeConfig.egress_proxy_url` at the configured lifecycle boundary. If
construction fails, return an unavailable runtime and register no lifecycle
handlers. Keep `DenyEgressClient` only for explicit disabled/test helpers.

Alternatives: leave the deny client and document adapter HTTP as unavailable
(rejected: it contradicts configured lifecycle availability), add a second
HTTP abstraction (rejected: duplicates the tested BridgeHost seam), or permit
ambient proxy discovery (rejected: weakens the explicit egress boundary).

Consequences: configured adapter HTTP now follows the same explicit proxy
contract as LLM and OIDC clients. Local bridge/runtime tests prove the wiring
and malformed construction failure; staging ACL, DNS/TLS, upstream reachability,
and EP-010 readiness evidence remain operator-owned.

### ADR-0056 Remediate event-listener unsoundness through the compatible lockfile

Context: `cargo audit` reported RustSec `RUSTSEC-2026-0221` for the transitive
`event-listener 5.4.1` dependency pulled through the vendored SQLx 0.8.6
stack. RustSec identifies `5.4.2` as the patched compatible floor.

Decision: update only the resolved lockfile entry to `event-listener 5.4.2`.
Do not add a direct forcing dependency, change the vendored SQLx boundary, or
add an advisory ignore. Verify the complete target graph and repository gates
after the update.

Alternatives: ignore the advisory (rejected: it is an unsoundness issue), add
a direct dependency solely to force resolution (rejected: unnecessary public
dependency surface), or replace/refresh SQLx (rejected: broad unrelated risk).

Consequences: the unsound event-listener warning is removed while the existing
SQLx, Store, and checked-query contracts remain unchanged. `fxhash` and
`proc-macro-error2` remain documented unmaintained transitive warnings, and
EP-010 still requires staging, recovery, operational, and human evidence.

### ADR-0057 Add explicit validated encrypted-vault recovery commands

Context: Hydra's age-encrypted vault already supported bounded documents,
atomic save, load, and owner key rotation, but deployment and package
contracts had no executable backup/restore path. Treating a raw file copy as
recovery would not validate the key or provide a safe restore boundary.

Decision: add `hydra-vault backup <destination>` and
`hydra-vault restore <source>`. Both commands validate the source artifact
with `HYDRA_VAULT_KEY` and copy ciphertext through the existing atomic file
boundary. Backup refuses an existing destination; restore requires
`HYDRA_VAULT_RESTORE_CONFIRM=restore` and may replace only the configured
vault artifact. Neither command prints plaintext, accepts keys as arguments,
touches CRM/Postgres/NATS, or runs during Kernel startup.

Alternatives: decrypt/re-encrypt during copy (rejected: increases plaintext
exposure and changes the owner artifact), expose an HTTP recovery endpoint
(rejected: unnecessary authority surface), or schedule automatic vault
backup/restore in Compose (rejected: owner-key custody and off-box policy are
not selected).

Consequences: local recovery mechanics and binary-level tests are executable,
while owner-key custody, off-box storage, capacity/retention, JetStream
snapshots, staged restore timing, and EP-010 human evidence remain open.

### ADR-0058 Make the Wasmtime feature boundary explicit

Context: `cargo audit` reported the unmaintained `fxhash` package through
Wasmtime's optional `profiling` default feature. Hydra uses the async
Component Model, Cranelift, runtime, standard-library, and WASI paths for its
checked WIT adapter ABI, but it does not use Wasmtime profiling or coredump
support.

Decision: keep Wasmtime and Wasmtime-WASI pinned at `36.0.13`, set the direct
`wasmtime` dependency to `default-features = false`, and explicitly enable
`async`, `component-model`, `cranelift`, `runtime`, and `std`. Add a mandatory
locked all-target graph check that rejects `fxhash` and
`fxprof-processed-profile`. Keep the pinned `age` library unchanged and
document its unrelated upstream `proc-macro-error2` maintenance warning.

Alternatives: retain Wasmtime defaults (rejected: carries an unused
unmaintained profiling path), disable Cranelift or the Component Model
(rejected: breaks the only permitted WIT adapter ABI), replace Wasmtime
(rejected: broad sandbox/runtime risk), or replace age (rejected: changes the
validated vault cryptographic boundary).

Consequences: the resolved graph is smaller and the avoidable `fxhash`
warning is removed without changing adapter behavior. The remaining age
maintenance warning and all EP-010 staging, recovery, operational, and human
evidence remain explicit residuals.

### ADR-0059 Upgrade the age vault library to remove the retired macro edge

Context: EP-047 removed Wasmtime's avoidable `fxhash` path, leaving
`proc-macro-error2` as the only unmaintained package reported by `cargo audit`.
The edge came from age `0.11.1` through `i18n-embed-fl 0.9.4`. Current age
`0.12.1` resolves the localization macro path through `i18n-embed-fl 0.10.1`
and `proc-macro-error3`.

Decision: pin the workspace age dependency to `0.12.1` with its empty default
feature set, refresh only the required lockfile graph, and add a mandatory
locked graph check rejecting age `0.11`, `i18n-embed-fl 0.9`, or
`proc-macro-error2`. Preserve the existing age artifact, named-secret JSON,
passphrase, rotation, backup, restore, and owner-output contracts.

Alternatives: keep age `0.11.1` (rejected: retains the only unmaintained
warning), replace age or custom-build cryptography (rejected: changes the
validated vault security boundary), or add an audit ignore (rejected: hides a
removable maintenance issue).

Consequences: local audit now reports no RustSec advisories or unmaintained
package warnings, while vault and CLI regression tests prove compatibility.
Owner-key custody, off-box protection, staged recovery, and EP-010 human
readiness evidence remain open.

### ADR-0060 Bound local recovery artifacts with explicit retention and vault scheduling

Context: EP-032 schedules validated Postgres archives into a local volume, and
EP-046/048 provide a validated encrypted-vault artifact contract, but neither
artifact class has a bounded scheduler/retention lifecycle. The repository has
no supported async-nats snapshot/restore API and must not copy a live NATS data
directory as if it were a valid recovery artifact.

Decision: add a preview-first `backup-retention.sh` helper that only considers
matching files in a configured directory and requires both
`HYDRA_BACKUP_RETENTION_APPLY=1` and `HYDRA_BACKUP_RETENTION_CONFIRM=prune` to
delete. Add a separate network-isolated `vault-backup` Compose profile that
invokes the existing `hydra-vault backup` binary with a read-only active vault,
operator-provided key, collision refusal, and captured child output. Keep
off-box replication, key custody, legal retention, JetStream snapshots, and
staging recovery as explicit operator/deployment work.

Alternatives: silently prune by age (rejected: malformed configuration could
destroy recovery artifacts), copy the live JetStream directory (rejected:
unsupported and potentially inconsistent), or add a cloud SDK (rejected:
invented credential/provider authority and new supply-chain scope).

Consequences: local artifact growth and vault scheduling now have executable
fail-closed boundaries, while local volumes are not treated as off-box backup,
and EP-010 remains partial until staging, ownership, and recovery evidence are
performed.

### ADR-0061 Replay canonical events from the Postgres outbox after JetStream loss

Context: The Postgres outbox is authoritative, but normal relay bookkeeping
does not re-emit rows already marked published after a broker volume loss or
stream replacement. The repository's async-nats boundary supports acknowledged
publish and stable message IDs but does not provide a supported snapshot/restore
operation for copying live JetStream files.

Decision: add a confirmation-gated `hydra-kernel --replay-events` command that
reads validated unparked outbox rows through Store-owned bounded SQL, publishes
them in ascending outbox-ID order through the existing JetStream publisher, and
preserves event IDs, subjects, payloads, and trace carriers. Require a
non-negative cursor and a maximum batch of 1000; stop on the first failure and
print only bounded counts/cursor metadata. Never mark rows published, create
claims, mutate CRM state, or replay action envelopes.

Alternatives: copy the NATS data directory (rejected: unsupported and
potentially inconsistent), mark replayed rows published (rejected: recovery
delivery is distinct from normal relay bookkeeping), or add an unbounded admin
endpoint (rejected: excessive authority and duplicate-load risk).

Consequences: an owner can resume bounded event delivery after broker loss and
consumers can deduplicate by stable event ID. The local round trip is not
staged broker-recovery evidence; off-box protection, snapshot/restore policy,
and EP-010 human readiness gates remain open.

### ADR-0062 Activation requires BridgeHost read-side conformance

Context: The governed prebuilt `deploy_adapter` handler already verifies a
digest-pinned component with `probe`, but a component could pass that narrow
check and enter the tenant registry's `active` state without exercising its
declared schema, list, or incremental-read contract.

Decision: After probe and before the `activating -> active` transition, run the
existing `BridgeLifecycle::conformance` boundary with the stored grant,
configuration, and digest, a fixed limit of 25, and no caller-selected kind.
On failure, persist a revision-checked `activating -> failed` transition and
return the executor failure path. Keep active redeploy idempotence and leave
generated code, autonomous canary, and promotion for a separate contract.

Alternatives: retain probe-only activation (rejected: the read-side contract
would remain unverified), invent a second canary abstraction (rejected: the
existing BridgeHost conformance boundary already owns these checks), or run
conformance after activation (rejected: an invalid adapter could be observed
as active).

Consequences: new prebuilt activations have a stronger local safety boundary
and durable failure evidence without a migration or dependency. The gate is
not staging/provider evidence and does not make EP-010 production-ready.

### ADR-0063 Make local performance evidence a shared required gate

Context: Hydra already had a release-only Governor p99 test, a named 10,000-
record bridge conformance soak, and a TOKENKILLER cache-hit audit, but
`verify.sh` did not run the first two and only conditionally ran the cache
audit. Nightly maintained a separate path, creating drift risk.

Decision: add `scripts/test-performance.sh` that runs the three existing checks
with strict success-marker validation, invoke it from `verify.sh` and nightly,
and make the release policy require the shared wrapper. Preserve the existing
5ms Governor threshold, 10k fixture, and `TK_HIT_RATIO_TARGET` default.

Alternatives: leave performance only in nightly (rejected: local verify could
pass after a regression), add a new benchmark dependency (rejected: existing
tests already own the contracts), or synthesize staging evidence (rejected:
local checks cannot prove staging behavior).

Consequences: every local verification now exercises the important release,
bridge-soak, and prompt-cache checks. The extra runtime is intentional; the
result remains local evidence and does not close EP-010 staging or human gates.

### ADR-0064 Use native disclosures for Shell no-JavaScript behavior

Context: EP-005 M5 remained unexecuted. The Shell's New Deal and Kind
Overrides controls used JavaScript `onclick` handlers and hidden CSS even
though their underlying forms already had native POST actions.

Decision: replace those show/hide controls with semantic `details`/`summary`
disclosures, add a skip link and explicit navigation landmark, and enforce a
dependency-free Rust contract test through `scripts/test-shell-accessibility.sh`.
Keep the existing URLs, CSRF fields, htmx enhancement, and server-rendered
forms unchanged.

Alternatives: add a browser automation dependency (rejected: this is a local
template contract and the repository forbids Node tooling), keep the
JavaScript-only controls (rejected: violates SPEC-004 progressive enhancement),
or claim a static contract is a staging accessibility review (rejected: it
cannot prove screen-reader, contrast, or real browser behavior).

Consequences: the two core disclosures are keyboard- and no-JavaScript-capable
by construction, while EP-010 retains the separate browser and human review
gates.

### ADR-0065 Bind deployment helpers to immutable artifacts and pinned SSH trust

Context: The release workflow captured the Buildx digest for attestation but
staging received only a mutable tag. The staging helper accepted a discarded
known-hosts file, while promotion skipped health validation if `curl` was
missing and pushed a mutable `latest-prod` alias.

Decision: pass the Buildx digest and owner-provided known-hosts record through
the release workflow; require digest validation and strict SSH host checking;
require both staging health and readiness plus Docker/curl for promotion; and
push only the immutable `${TAG}-prod` image. A missing `PROMOTE=yes` remains a
safe dry-run with a distinct marker.

Alternatives: keep tag-only deployment (rejected: tags can move), retain
`accept-new` with `/dev/null` (rejected: it does not authenticate the staging
host), skip readiness or curl failures (rejected: fail-open release safety),
or keep `latest-prod` (rejected: it weakens rollback and provenance).

Consequences: local release tooling has a stronger, truthful safety boundary;
actual tag runs still require owner-controlled digest, registry, SSH, staging,
and human authorization evidence.

### ADR-0066 Publish only immutable Hydra release tags

Context: EP-054 bound staging and production promotion to a Buildx digest and
removed `latest-prod`, but the tag-triggered release workflow still published
`hydra/kernel:latest`. That contradicted the immutable image contract in
deployment, rollback, and package documentation.

Decision: remove only the Hydra `:latest` tag from the release Buildx step and
add negative policy checks. Keep local Compose's `HYDRA_TAG` and
`HYDRA_TAG=local` development behavior unchanged.

Alternatives: retain `latest` for convenience (rejected: it is mutable and
weakens rollback/provenance), change local Compose defaults (rejected: it is a
separate development concern), or publish an alias from an operator job
(rejected: that requires a separate reviewed release policy).

Consequences: release consumers must select an explicit version or digest;
local development remains compatible, and actual registry/tag-run evidence is
still required before production readiness.

### ADR-0067 Use async-NATS credentials files and required TLS outside dev

Context: The Kernel and event-replay CLI used plain `async_nats::connect`,
while the deployment contract allowed shared or remote NATS. The workspace
already depended on async-nats 0.49.1 but had disabled its existing `nkeys`
feature, so the verified credentials-file builder was unavailable.

Decision: enable the existing `nkeys` feature, load credentials only from a
mounted `NATS_CREDS_FILE`, reject credentials embedded in `NATS_URL`, and
require both authentication and TLS in staging and production. Optional CA
and mTLS files are supported through explicit paths. Development and loopback
tests retain plain NATS defaults.

Alternatives: keep an unauthenticated private broker (rejected: insufficient
for shared/remote deployment), put user/password in `NATS_URL` (rejected:
secret-bearing URLs leak through diagnostics), or add a new NATS client crate
(rejected: the existing client already supplies the required API).

Consequences: secure deployments need operator-mounted credentials and trust
anchors, local Compose development remains compatible, and no broker,
certificate, or secret material is created by Hydra.

### ADR-0068 Hash local session bearer tokens at rest

Context: the local session repository previously stored opaque bearer tokens
in plaintext in `hydra_session`. A database read exposure would therefore
immediately become session impersonation, even though the tokens are random.

Decision: add an additive `token_hash` column and partial unique index. New
sessions store only a lowercase SHA-256 digest; lookup, touch, and revoke
derive the same digest before querying. Existing plaintext rows remain
temporarily readable only through possession of the presented token and are
upgraded atomically on lookup, with the plaintext column cleared. The
session-token schema constraint permits exactly one credential representation
during this compatibility window.

Alternatives: keep plaintext storage (rejected: unnecessary bearer exposure),
backfill all legacy tokens in SQL (rejected: requires retrieving bearer
secrets or adding a database crypto extension), or break all existing sessions
at migration time (rejected: avoidable compatibility and operator disruption).

Consequences: new database exposures do not reveal active local sessions;
legacy rows converge as they are used and expire naturally. A staging
deployment should revoke or allow all legacy sessions to expire after the
migration, and session-store recovery evidence remains an EP-010 operator
gate.

### ADR-0069 Keep the legacy HMAC token helper development-only and fail closed

Context: `fabric::auth::jwt::TokenService` is retained for isolated local
fixtures, while the HTTP Nexus boundary uses asymmetric OIDC validation. The
helper previously verified only the HMAC bytes and deserialized claims, so a
future caller could accidentally accept a non-HS256 header or malformed time
claims.

Decision: retain the helper for development/tests only, require the exact
`HS256`/`JWT` header, require the `hydra` issuer and non-empty subject, reject
expired or materially future-issued tokens, require expiration after issued-at,
and cap verification input at 8 KiB. Production authentication remains on the
OIDC resource-server path and does not use this helper.

Alternatives: remove the helper immediately (rejected: existing deterministic
fixtures still use it), or expand it into a second production token system
(rejected: that would duplicate and weaken the asymmetric OIDC boundary).

Consequences: local fixtures remain compatible while accidental legacy-helper
reuse fails more safely. The helper is not a production identity provider, and
its HMAC secret must never be treated as a production credential.

### ADR-0071 Bound external business binding identifiers at every write and lookup boundary

Context: The Hydra-owned external binding tuple was unique and lifecycle-safe,
but its provider, external tenant, and external business text fields were
unbounded and accepted control characters in Rust and Postgres. Signed OIDC
claims reached binding lookup without the same explicit size contract.

Decision: Define one Store-owned invariant: binding text is non-empty, free of
control characters, and at most 512 Unicode scalar values. Enforce it when
creating and resolving bindings, when validating the configured OIDC provider
and external claims, and with an additive Postgres check constraint. Nil Hydra
tenant IDs fail closed for binding listing. No external ID becomes tenant
authority, and no delete or replacement path is introduced.

Alternatives: rely on TEXT and token-size limits alone (rejected: those do not
protect direct Store callers or log-safe identifiers), normalize/truncate IDs
(rejected: it can collide distinct external principals), or add a second
binding abstraction (rejected: Hydra's existing binding repository remains the
canonical boundary).

Consequences: malformed or oversized binding material fails before lookup or
durable storage, while valid Unicode identifiers remain compatible. The
additive migration requires existing binding rows to satisfy the same
contract; no production database was touched during validation.

### ADR-0070 Enforce RFC3339 timestamps at the canonical event boundary

Context: The canonical event JSON Schema declared `date-time` fields, but the
runtime `HydraEventEnvelope::validate()` path only checked that timestamps were
non-empty text. Store outbox loading and durable Nexus consumers use runtime
validation, so malformed timestamps could otherwise pass those paths.

Decision: Reuse the workspace `time` crate with its existing RFC3339 formatter
and enable its parsing feature for CDM. Validate `occurred_at` and optional
`observed_at` with the same parser used by the schema contract, while retaining
the existing bounded, control-free text checks. Do not change event names,
schema versions, stored timestamp representation, or migration history.

Alternatives: keep text-only validation (rejected: runtime and schema would
disagree), add a new date-time dependency (rejected: the workspace already
uses `time`), or parse timestamps in Store only (rejected: durable consumers
and other CDM callers must share the canonical boundary).

Consequences: malformed canonical timestamps fail closed before event-log,
outbox, or consumer processing. The change is additive to the code contract,
does not require a migration, and remains local evidence rather than staging
consumer compatibility evidence.
