# EP-012 Nexus Auth and MCP Control Plane

Plan status: COMPLETE

## 1. Purpose / Big Picture
Replace Hydra's development external boundary with a production-grade OAuth/OIDC resource-server path, explicit external business binding, centralized authorization, one typed capability registry, MCP 2025-11-25 Streamable HTTP behavior, and a compact versioned Nexus REST facade.

## 2. Scope
Asymmetric JWT/JWKS or pinned-key validation; cached trust material; generic principal context; external binding migration/repository; scope authorization; effective rate limiting; MCP Origin/version/GET+POST/schema behavior; canonical read/proposal tools; `/v1/nexus/` read facade; deterministic local issuer fixtures and boundary tests.

## 3. Non-goals
No direct execution, idempotency store, immutable approval persistence, canonical JetStream event envelope, fake full Nexus E2E, GraphQL, A2A, Nexus model provider, production deployment, or live external identity dependency. Approval endpoints may be made fail-closed here; durable approval execution lands EP-013.

## 4. Context and Orientation
EP-011 proves current tenant authority comes from MCP `_meta`/`x-hydra-tenant`, JWT/token behavior is fixed-secret development code, MCP approval fabricates anonymous identity, entity routes allow unauthenticated CRUD, and rate limiting is a pass-through. SPEC-010 sections 4 through 10 are controlling. Authenticate first, then resolve a Hydra-owned binding, then authorize a capability.

## 5. Files to Read First
`NEXUS_INTEGRATION_AUDIT.md`; `NEXUS_INTEGRATION.md`; `.agent/specs/SPEC-010-nexus-interoperability.md`; `SECURITY.md`; `ENVIRONMENT.md`; `COMMANDS.md`; `crates/fabric/src/auth/{mod.rs,jwt.rs,role.rs,session.rs}`; `crates/fabric/src/{mcp.rs,rate.rs,services.rs,lib.rs}`; `crates/fabric/src/rest/{mod.rs,oauth.rs,openapi.rs,entities.rs,envelopes.rs}`; `crates/store/src/{lib.rs,auth.rs}`; `crates/kernel/src/{config.rs,main.rs}`; `migrations/0007_auth.sql`; current Fabric tests.

## 6. Files to Change
`Cargo.toml`; `Cargo.lock`; `deny.toml` only if license policy needs an evidence-backed additive rule; `DECISIONS.md`; `COMMANDS.md`; `ENVIRONMENT.md`; `SECURITY.md`; `NEXUS_INTEGRATION.md`; `crates/store/src/lib.rs`; `crates/store/src/external_bindings.rs` (new); `migrations/0008_external_tenant_binding.sql` (new); `.sqlx/` query metadata generated for new checked queries; `crates/fabric/Cargo.toml`; `crates/fabric/src/lib.rs`; `crates/fabric/src/auth/mod.rs`; `crates/fabric/src/auth/jwt.rs`; `crates/fabric/src/auth/principal.rs` (new); `crates/fabric/src/auth/oidc.rs` (new); `crates/fabric/src/auth/authorization.rs` (new); `crates/fabric/src/capabilities.rs` (new); `crates/fabric/src/mcp.rs`; `crates/fabric/src/rate.rs`; `crates/fabric/src/services.rs`; `crates/fabric/src/rest/mod.rs`; `crates/fabric/src/rest/nexus.rs` (new); `crates/fabric/src/rest/oauth.rs`; `crates/fabric/src/rest/openapi.rs`; `crates/kernel/src/config.rs`; `crates/kernel/src/main.rs`; `crates/store/tests/integration_external_bindings.rs` (new); `crates/fabric/tests/nexus_auth.rs` (new); `crates/fabric/tests/mcp_contract.rs` (new); `crates/fabric/tests/fixtures/mcp-tools.snapshot.json` (new); this plan and `.agent/state/execplan-index.md`.

## 7. Interfaces and Contracts
Implement SPEC-010 `PrincipalType`, `PrincipalContext`, binding semantics, stable scopes, and one `CapabilityDescriptor` registry. JWT validation MUST be asymmetric and validate signature/algorithm/issuer/audience/exp/nbf/jti when present. MCP targets `2025-11-25`, validates Origin and request size, negotiates protocol version, supports required GET/POST transport behavior, returns `structuredContent`, and never reads tenant authority from request metadata. `/v1/nexus/` initial GET endpoints are capabilities/context/bindings/events-status.

## 8. Milestones
M1 Dependency/SDK decision and generic identity types. Evaluate the official Rust MCP SDK against protocol support, Axum fit, layer law, license, `cargo audit`, and `cargo deny`; adopt or record rejection in ADR-0019 follow-up. Implement principal/config skeleton. Validation: `cargo check -p fabric`. Expected: exit 0. Recovery: dependency conflict -> inspect `cargo tree -p fabric`, prefer existing/std or hardened custom MCP path, record decision before continuing.

M2 Binding persistence and resource-server validation. Add migration/repo, cached JWKS/pinned-key resolver, claim checks, and fail-closed binding resolution. Validation: `cargo test -p fabric nexus_auth -- --nocapture`. Expected: all anonymous/expired/issuer/audience/signature/binding/cross-business cases pass. Recovery: isolate token fixture validation from DB binding tests; never weaken a claim to make a fixture pass.

M3 Authorization and capability registry. Centralize scope/principal/tenant checks, make rate limiting effective, define schemas/availability, and route existing safe aliases through the registry. Validation: `cargo test -p fabric capability -- --nocapture`. Expected: deterministic registry and scope tests pass, duplicate names fail. Recovery: if definitions diverge, remove endpoint-local copies and derive all surfaces from the registry.

M4 MCP and Nexus REST facade. Implement transport/version/Origin/auth/structured output, canonical tools, actual search filtering, and compact deterministic REST projections. Remove development tenant fallback from non-test code and reject external direct CRUD without valid local authorization. Validation: `cargo test -p fabric mcp_contract -- --nocapture`. Expected: contract snapshots and boundary cases pass. Recovery: narrow to one JSON-RPC method or REST handler; preserve safe aliases only through the same authenticated code.

M5 Persistence/security/docs/final gates. Refresh SQLx metadata, update environment/security/integration docs, run dependency/security gates and full verification. Validation: `bash scripts/dependency-audit.sh` then `bash scripts/security-check.sh` then `bash scripts/verify.sh`. Expected: `dependency audit: ok`, `security check: ok`, `verify: ok`. Recovery: new dependency advisory/license failure requires removal or a documented safer alternative, never a blanket ignore.

## 9. Concrete Steps
Complete M1-M5 in order. Use deterministic local asymmetric issuer material encoded as test data without committing a PEM private-key marker. Authenticate before binding, bind before authorization, authorize before dispatch. Update Progress and Decision Log after every command. Activate EP-013 only after all acceptance passes.

## 10. Validation and Acceptance
All EP-012 tests listed by the master directive pass; no external call can choose tenant; invalid identity/binding fails closed; agent approval is denied; schemas are stable; search honors query; capability/REST/MCP surfaces share one registry; existing safe compatibility is preserved or explicitly deprecated; dependency/security/verify gates are green.

## 11. Idempotence and Recovery
Migration is additive and rerunnable through SQLx. JWKS refresh replaces cached key sets atomically and pinned-key mode is offline. Binding bootstrap uses unique constraints. Snapshot updates are intentional and reviewed. On interruption, use plan Progress and run the narrowest milestone test; never re-enable development fallback outside `cfg(test)`.

## 12. Progress
- [x] M1 - Official SDK adopted; generic identity/config types compile; audit and deny remain green
- [x] M2 - Binding persistence and resource-server validation; Store tests, 14 auth tests, RustSec, and deny policy pass
- [x] M3 - Central authorization, effective shared rate limits, and one deterministic capability registry
- [x] M4 - MCP 2025-11-25 and Nexus REST facade
- [x] M5 - Metadata, docs, audits, and full verification

## 13. Surprises & Discoveries
- 2026-08-10: Official `rmcp` 3.1.2 supports both the current MCP release and `2025-11-25`, permits narrowing supported protocol versions, carries Axum request extensions into tool context, and implements Host/Origin/body-size/cancellation behavior without an Axum version dependency.
- 2026-08-10: The first kernel config compile used a Rust 2024 let-chain in Hydra's Rust 2021 edition. Replacing it with nested conditions was the only required compatibility correction.
- 2026-08-10: The minimal rmcp feature graph adds a second `base64` release but no vulnerability, denied license, forbidden source, or denied advisory. Existing `fxhash` and `event-listener` warnings are unchanged from EP-011.
- 2026-08-10: The first `jsonwebtoken` 11.0.0 backend selection (`rust_crypto`) pulled vulnerable `rsa` 0.9.10 and failed `cargo audit` on `RUSTSEC-2023-0071`, which has no fixed release. Hydra did not add an ignore; the backend was changed to `aws_lc_rs` and must re-pass the same auth and dependency gates.
- 2026-08-10: `jsonwebtoken` 11.0.0 with AWS-LC removed the advisory but its native Windows build failed because NASM and a fully initialized MSVC environment were unavailable. The narrower reproducible path is pinned 9.3.1 with its existing `ring` backend; this must pass both executable tests and dependency gates before M2 closes.
- 2026-08-10: The first MCP search implementation applied the requested result limit before filtering, which could miss a later matching record. The contract fixture put a nonmatch first and proved the corrected bounded scan honors the query.
- 2026-08-10: The shared rate limiter compiled and passed unit tests but was not initially connected to route dispatch. M4 wired it after authentication for both external principals and local sessions and added an HTTP 429 contract test.
- 2026-08-11: The first full M5 run found a real `clippy::items_after_test_module` failure in `auth/mod.rs`; moving the re-export resolved it and the narrower lint gate passed. The next full run exceeded the 15-minute outer tool timeout while `build.sh` was still compiling, so it was not counted. The unchanged warm rerun completed in 531.1 seconds with exit zero.

## 14. Decision Log
| Date | Decision | Rationale |
|---|---|---|
| 2026-08-10 | Queue behind EP-011 | Trust-boundary implementation starts only after reality reconciliation passes |
| 2026-08-10 | Adopt pinned official `rmcp` 3.1.2 with minimal server transport features | Apache-2.0, MSRV, protocol compatibility, Tower/Axum fit, and request-context propagation satisfy the required SDK evaluation; Hydra retains auth and business authority |
| 2026-08-10 | Require Nexus trust and Origin configuration only when integration is enabled | Preserves independently deployable standalone mode while making the external boundary fail closed when enabled |
| 2026-08-10 | Use `jsonwebtoken`'s AWS-LC provider instead of its RustCrypto provider | The RustCrypto feature introduces an unfixed RSA advisory; removing the vulnerable dependency is safer than suppressing RustSec |
| 2026-08-10 | Pin `jsonwebtoken` 9.3.1 with `ring` after the AWS-LC build diagnostic | It preserves required asymmetric/JWK APIs, avoids the vulnerable RSA crate, and fits Hydra's existing reproducible Windows toolchain without a machine-wide NASM install |
| 2026-08-10 | Resolve external authority only through an active Hydra-owned binding after JWT validation | A signed caller supplies external business identity, but cannot supply or override the resulting Hydra tenant; disabled, revoked, missing, and cross-business mappings fail closed |
| 2026-08-10 | Advertise unavailable canonical capabilities with explicit reasons instead of omitting or faking them | Discovery remains stable while proposal idempotency, tenant-scoped envelope get, and canonical event timeline wait for EP-013/EP-014 runtime support |
| 2026-08-10 | Map local Hydra roles to the same stable scope set used by external principals | Local and Nexus paths can converge on one authorization service without removing the existing standalone role hierarchy |
| 2026-08-10 | Keep safe legacy MCP read names only as authenticated deprecated aliases | Compatibility does not restore tenant metadata authority, approval, TOKENKILLER stats, or any direct mutation tool |
| 2026-08-10 | Disable the historical token endpoint instead of issuing development credentials | Hydra is a resource server; token issuance and signing authority belong to the configured Nexus identity provider |
| 2026-08-10 | Expand the M4 file surface to local REST handlers and shared tests | Installing one authenticated `AuthCtx` at middleware required handlers to consume request extensions, and existing tests had to declare their development identity explicitly; no route bypass was retained |

M2 evidence (2026-08-10): migration `0008_external_tenant_binding.sql` applied to isolated Postgres; `cargo test -p store --test integration_external_bindings -- --nocapture` -> 2 passed; `cargo test -p fabric nexus_auth -- --nocapture` -> 14 passed; `cargo audit` -> no vulnerabilities with the two pre-existing allowed warnings; `cargo deny check` -> advisories, bans, licenses, and sources all ok. The unsuccessful RustCrypto audit and AWS-LC Windows build are retained above and were not presented as passing evidence.

M3 evidence (2026-08-10): `cargo test -p fabric capability -- --nocapture` -> 13 passed, covering deterministic order, duplicate/unsafe-alias rejection, availability truth, tenant/scope checks, read-only proposal denial, agent/four-eyes approval rules, local-role scope mapping, and limiter enforcement. `cargo check -p hydra-kernel` -> exit 0 with only the pre-existing vendored SQLx cfg warnings and temporary unused Nexus config warnings pending M4 runtime wiring; the real kernel now constructs shared limiter state and socket connect information.

M4 evidence (2026-08-10): `cargo test -p fabric mcp_contract -- --nocapture` -> 4 passed after HTTP limiter wiring and schema snapshot pinning. The suite uses the actual `rmcp` Streamable HTTP service with deterministic Ed25519/OIDC fixtures and proves anonymous rejection plus RFC 9728 challenge, Origin enforcement, exact `2025-11-25` negotiation, authenticated GET conformance, deterministic nine-tool discovery, stable schema digest, real bounded search filtering, output-schema validation, metadata/header tenant override resistance, safe alias deprecation, read-only proposal denial, compact REST context, binding projection, correlation response, rejection of the local direct-write route, and an enforced HTTP 429 with `Retry-After`.

M5 evidence (2026-08-11): `cargo sqlx prepare --workspace -- --all-targets` wrote four checked binding-query metadata files; `cargo test -p fabric` against isolated Postgres -> 128 passed across 10 suites; `bash scripts/dependency-audit.sh` -> `dependency audit: ok`; `bash scripts/security-check.sh` -> `security check: ok` with only the two pre-existing allowed `fxhash`/`event-listener` warnings; narrower `bash scripts/lint.sh` -> `lint: ok` after the named Clippy correction. The authoritative unchanged `bash scripts/verify.sh` rerun against isolated Postgres `127.0.0.1:55432` and NATS `127.0.0.1:54222` exited zero in 531.1 seconds and reached the script's `verify: ok` terminal path. No production deployment or external identity service was used.

## 15. Outcomes & Retrospective
EP-012 is complete. Hydra now acts as a configurable asymmetric OAuth/OIDC resource server for Nexus, resolves one active Hydra-owned external business binding before authorization, exposes nine registry-derived MCP 2025-11-25 tools plus safe authenticated aliases, serves the four read-only `/v1/nexus/` projections, enforces Origin/body/rate limits, and keeps tenant authority out of headers and MCP metadata. The historical fixed-secret token endpoint is disabled rather than presented as production OAuth.

Backward compatibility is limited deliberately: standalone local role/session APIs remain, the explicit development admin token is available only when the kernel runs in development mode, and safe legacy MCP read aliases use the same authenticated path. Legacy external approval, TOKENKILLER-stat, and direct mutation tools were not retained because they violated the trust boundary.

Remaining work is intentionally owned by EP-013 through EP-015: proposal idempotency and execution, durable approval assertions, runtime handler/agent wiring, canonical JetStream events/tracing, real E2E gates, and deployment topology. EP-010 remains only partially complete. No production deployment occurred.
