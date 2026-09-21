# PRODUCTION_READINESS.md

## Current Status

**NOT PRODUCTION READY. EP-010 remains PARTIAL.** Hydra is production-ready only when `HYDRA_TEST_DATABASE_URL=<disposable-loopback-url> NATS_URL=<loopback-test-broker-url> bash scripts/production-readiness-check.sh` emits the exact terminal signal `production-readiness:ok`, every required staging drill and review has dated evidence, and the human launch sign-off is complete. The wrapper rejects missing or non-loopback test database or broker targets before running nested gates. Nexus interoperability passing locally does not satisfy those gates.

## Verified Local Evidence (2026-08-11)

| Gate | Current evidence | Scope |
|---|---|---|
| Preflight | `preflight: ok` | Local repository/tooling |
| Unit | `unit tests: ok` | Workspace unit tests |
| Integration | `integration tests: ok`; `failure suites: ok` | Isolated local Postgres and JetStream |
| Nexus E2E | Three named tests passed; `e2e tests: ok` | Deterministic fake issuer/JWKS and fake Nexus only |
| Security | `security check: ok` | Local source/dependency scan; no RustSec vulnerabilities remain, while two upstream unmaintained-package warnings are documented for owner review |
| Dependency policy | `advisories ok, bans ok, licenses ok, sources ok`; `dependency audit: ok` | Current Cargo.lock and deny policy |
| Container image | `hydra/kernel:local` built; adapter emitted `adapters: ok`; non-root image layout passed | Local Docker Desktop only |
| Compose | Standalone and Nexus-connected config exited 0; only Caddy publishes ports | Configuration validation only; no staging deployment |
| NATS transport boundary | Focused Kernel/NATS tests, `nats policy: ok`, and the resource-safe full verifier passed; staging/production require credentials-file authentication plus TLS while loopback development remains compatible | Local transport and fail-closed configuration evidence only; no staged broker account, certificate-chain, rotation, or recovery evidence |
| Canonical event payload bounds | CDM event-contract suite passes 7 tests covering schema/runtime agreement, nested text bounds, control-character rejection, typed payload validation, and nonzero entity versions | Local contract and schema evidence only; no staged consumer compatibility or replay drill |
| Canonical event timestamps | CDM event-contract suite passes 8 tests; runtime validation now enforces RFC3339 for `occurred_at` and `observed_at` consistently with the published schema | Local contract and schema evidence only; no staged consumer compatibility or replay drill |
| External binding identifier boundary | Store and Fabric tests plus migration `0024_external_binding_text_bounds.sql` enforce non-empty, control-free, <=512-Unicode-scalar provider, external tenant, and external business identifiers; nil Hydra tenant listings fail closed | Local code and disposable-schema evidence only; no staged identity/bootstrap, binding rotation, or revocation drill |
| Full verifier | Exit 0 through the terminal `verify: ok` path after EP-029 changes, using isolated loopback Postgres and NATS | Complete local verifier only |
| Shell/local REST hardening | Shell 2 tests, Fabric auth 27 tests, and strict Shell clippy passed; local REST derives tenant from a verified session | Local executable evidence only; no browser/staging review |
| Environment-truthful Shell login | Shell package tests prove development-only banner/placeholders render only when `allow_development_identity` is enabled; the global Store-backed limiter remains the pre-auth admission boundary | Local executable evidence only; no browser/staging review |
| Integration target safety | Non-loopback database URL rejected before test execution; isolated loopback run printed `integration tests: ok` and `failure suites: ok` | Test-harness safety only |
| Store-only SQL enforcement | `bash scripts/security-check.sh` scans all production Rust source under `crates/fabric/src`, `crates/admin-cli/src`, and `crates/kernel/src`; the scheduler concurrency test uses the Store API rather than direct SQL | Local static/source and disposable-schema evidence only; no production deployment or external operator evidence |
| Parked-event readiness boundary | Relay health loads the durable parked-outbox count at startup and `/readyz` fails closed with `canonical_event_relay_parked_event`; restart-shaped integration coverage passes | Local failure-mode evidence only; operator repair and staged recovery remain EP-010 gates |
| Session token at-rest boundary | Disposable Store integration test proves new bearer sessions persist only SHA-256 digests, and legacy plaintext rows upgrade and clear on presenting-token lookup | Local schema and compatibility evidence only; staging expiry/revocation and session-store recovery evidence remain open |
| Encrypted vault | Age-backed named-secret vault round trip, wrong-key/tamper rejection, CLI name-only output, validated ciphertext-preserving backup/restore, kernel valid-load, and staging/prod missing-file fail-closed tests passed in EP-018/EP-046 | Local synthetic values only; no owner-key custody, off-box protection, or staging restore drill |
| Readiness gate | `production-readiness:FAIL: drill D1 — no PASS row in OPERATIONS.md` after local gates passed | Correct fail-closed result; staging evidence is absent |
| Operational helper safety | `bash scripts/test-operational-tools.sh` -> `operational tools: ok` | Fake PostgreSQL clients only; no restore drill or production database access |
| Structured readiness | Isolated smoke confirms `/readyz/details` returns `status=ready` with Postgres, NATS, events, and bridge-lifecycle checks | Local endpoint contract only; no staging outage exercise |
| Durable execution recovery | Store and Kernel tests recover a durable `Approved` envelope without an in-memory token; stale `Executing` work is reported fail-closed | Local restart-style evidence only; no staging crash/rollback drill |
| Autonomy freeze control | Durable tenant freeze overlay, revision-invalidated Governor cache, owner confirmation gate, thaw restoration, idempotence, and event coverage pass locally | Code-level D5 control only; no staging freeze drill or human evidence |
| Observability profile | `bash scripts/check-observability.sh` -> `observability policy: ok`; observability Compose config exits 0 | Internal configuration and rule-wiring evidence only; no live receiver, dashboard, or staging alert drill |
| Scheduled backup profiles | Backup and network-isolated vault-backup Compose configs; operational suite proves scheduler once-mode, interval rejection, helper-failure propagation, preview/apply retention, and vault key non-disclosure | Local named-volume artifacts only; no off-box copy, legal retention policy, restore drill, JetStream snapshot, or staging evidence |
| Authoritative event replay | Store selection, confirmation-gated Kernel CLI, and disposable Postgres/JetStream round trip pass; repeated event IDs are deduplicated and outbox state remains unchanged | Local recovery contract only; no staged broker-loss recovery, consumer rebuild, or operator sign-off |
| Local performance evidence | `bash scripts/test-performance.sh` passes the release Governor p99 test, named `c9_soak_10k` bridge conformance, and TOKENKILLER cache audit | Local executable thresholds only; no staging shell/API latency, 24-hour soak, live provider budget/cache review, or human performance sign-off |
| Shell accessibility contract | `bash scripts/test-shell-accessibility.sh` proves Shell landmarks, native disclosures, and native POST fallbacks | Static template evidence only; no browser keyboard, screen-reader, contrast, no-htmx, or staging review |
| Deployment helper safety | `bash scripts/test-deployment-safety.sh` and `bash scripts/check-release-policy.sh` pass digest/SSH/readiness/immutable-promotion policy checks | Local helper/workflow policy only; no tag run, registry publication, SSH connection, staging deployment, or production promotion |

## Verified Nexus Controls

- Nexus calls use asymmetric JWT validation for signature, issuer, audience, time claims, allowed algorithms, scopes, principal type, and active Hydra-owned business binding.
- Caller headers, MCP `_meta`, and tool arguments cannot select a Hydra tenant.
- External mutations use canonical capabilities, validated input, idempotent ActionEnvelope proposals, deterministic Governor decisions, typed execution, durable receipts, audit, and outbox events.
- Agents cannot approve; approvals require a distinct human-delegated principal, accepted authentication strength, scope, tenant match, and immutable approval assertion.
- The canonical event relay waits for a JetStream acknowledgement before marking an outbox row published; Postgres remains authoritative.
- The fake Nexus round trip proves discovery, compact context, search, queued stage change, approval, execution, verification, canonical event delivery, retry deduplication, and cross-business denial.

## Open Production Gates

| Area | Required evidence still missing |
|---|---|
| Staging | Reproducible tag deployment with real DNS/TLS and synthetic tenants |
| D1-D5 | Restore, rollback, NukeGuard alert, cache, and autonomy-freeze drills with dated PASS rows |
| Soak | 24-hour staging soak with error, resource, and TOKENKILLER cache evidence |
| Security | Review of the last five PRs, live boundary testing, secret-management implementation review, and disposition of allowed advisories |
| Privacy/data | Local admin-only export and non-destructive tenant retention preview are implemented and tested; artifact retention is separate and explicit; staging demonstration, owner retention policy, and privacy review remain missing |
| Performance | Staging shell/API latency, sustained agent budget/cache results, and human performance review; local Governor p99, 10k bridge soak, and TOKENKILLER cache evidence now pass through EP-052 |
| Accessibility | Browser keyboard, screen-reader, contrast, and no-JavaScript core-flow review; local Shell semantic/fallback contract now passes EP-053 |
| Observability | Live dashboards, alert delivery, event-infrastructure outage response, and redacted-log inspection |
| Recovery | Proven Postgres restore, JetStream recovery, encrypted-secret recovery, and old-image/new-schema rollback |
| Release | Workflow SBOM/provenance policy, signed tag-run attestation, immutable image/version ownership, supported upgrade evidence |
| Human control | Named security/privacy/accessibility/operations owners and final launch sign-off |

## Known Runtime And Packaging Gaps

- `HYDRA_VAULT_KEY` and `HYDRA_VAULT_PATH` now load a persisted age-encrypted vault into the production kernel path; EP-046 adds locally validated ciphertext-preserving `hydra-vault backup` and confirmation-gated `restore`, while owner-key custody, off-box protection, and staged recovery remain open. Prebuilt bridge deploy/pause/resume and the bounded manual incremental/full-relist sync handler are code-wired behind `HYDRA_ADAPTERS_PATH` and the configured SecretSource. Owner-controlled scheduling is also code-wired but disabled by default. New deployment is probe- and conformance-gated before `active`; staging/provider validation, mapping synthesis activation, autonomous canary, and promotion remain open.
- The checked-in Caddyfile uses an internal CA and is local/reference configuration, not production TLS.
- The reference NATS network is private and not host-published, but a remote/shared deployment still needs operator-owned private networking and broker authentication.
- External binding and active-operator bootstrap now use the local, confirmation-gated `hydra-admin` binary; it is not a public endpoint, does not run migrations, and direct SQL remains forbidden. Owner credential custody, staging identity evidence, and recovery remain open.
- The production rate limiter is Store-backed with atomic Postgres windows and fail-closed authority errors; local multi-replica staging quota behavior and operational outage evidence remain unperformed.
- Session and CSRF cookies are HttpOnly and SameSite=Lax; Kernel adds `Secure` in staging and production. Browser validation, session-store recovery, and live deployment evidence remain open.
- The optional `backup` Compose profile schedules the existing atomic Postgres backup helper and can run preview-first artifact retention with two explicit apply controls. The separate `vault-backup` profile schedules validated ciphertext-preserving copies with `network_mode: none`. There is still no off-box replication, legal retention policy, export-delivery, JetStream restore path, or staging recovery evidence. Tenant export and retention preview are read-only and do not establish a legal retention policy.
- The historical `admin` database seed is preserved for auditability but is marked `development_seed` and disabled by migration `0016_auth_seed_hardening.sql`; `SessionStore` rejects it in every environment. The local owner tool creates `auth_source=operator` users and can disable/re-enable them while revoking active sessions. Staging identity evidence, credential custody, and recovery remain open.
- Approved execution dispatch now has a bounded Postgres recovery scan after Kernel startup; an `Executing` envelope older than 15 minutes fails readiness and is not automatically replayed. Operator resolution and staging crash/recovery evidence remain open.
- The release workflow now defines explicit BuildKit SBOM/provenance settings and signed digest-attestation policy, guarded by `scripts/check-release-policy.sh`; no authorized tag-run attestation, registry verification, or supported-upgrade evidence has been recorded locally.

## Supply-Chain Residuals

EP-047 removes `RUSTSEC-2025-0057` (`fxhash`, unmaintained) from the resolved graph by disabling Wasmtime's unused default profiling feature; the mandatory feature gate prevents its return. EP-045 removed `RUSTSEC-2026-0221` (`event-listener`, unsound) without adding an advisory ignore. EP-048 upgrades the pinned age vault to `0.12.1`, whose resolved `i18n-embed-fl 0.10.1` edge removes `RUSTSEC-2026-0173` (`proc-macro-error2`). Current local audit reports no RustSec advisories or unmaintained package warnings; `cargo deny` still reports warning-only duplicate versions and unused license allowances. The gates pass, but production review still requires human acceptance of the remaining non-advisory residuals.

## Post-Audit Store Boundary Verification (2026-08-13)

The scheduler concurrency/idempotency test no longer uses direct SQL, and the
security gate now scans every production Rust source file in Fabric, the admin
CLI, and Kernel for SQL or pool access outside Store. The focused scheduler
suite passed 3 tests, `bash scripts/security-check.sh` passed, and the full
repository verifier reached `verify: ok` against isolated loopback services.
This is local implementation evidence only; it does not satisfy EP-010's
staging, recovery, soak, review, or human sign-off requirements.

The relay now also treats durably parked canonical outbox rows as a readiness
failure across process restarts. Transient JetStream publication failures still
retry; only contract-invalid parked rows require operator recovery.

## Post-EP-045 Current Verification (2026-08-12)

The lockfile resolves `event-listener v5.4.2` through the existing vendored
SQLx 0.8.6 stack. `cargo tree -i event-listener --target all --offline`,
`cargo audit`, `cargo deny check`, `bash scripts/security-check.sh`, and
`bash scripts/dependency-audit.sh` all passed through the repository's working
Git Bash environment. Audit reports only the two unmaintained warnings above;
no vulnerability or `RUSTSEC-2026-0221` result remains. This is local
dependency evidence only and does not change EP-010's partial production
readiness status.

## Final Launch Gate

| Check | Owner | Date | Result |
|---|---|---|---|
| production-readiness-check.sh | djw | TBD | BLOCKED: EP-010 evidence incomplete |
| Restore drill (D1) | djw | TBD | PENDING: staging required |
| Rollback drill (D2) | djw | TBD | PENDING: staging required |
| Nuke/cache/autonomy drills (D3-D5) | djw | TBD | PENDING: staging required |
| 24h staging soak | djw | TBD | PENDING: staging required |
| Security review | djw | TBD | PENDING: local scan is not human review |
| Performance review | djw | TBD | PENDING: no staging measurements |
| Privacy/data review | djw | TBD | PENDING: staging export/retention demonstration and owner policy review |
| Accessibility review | djw | TBD | PENDING: no executed audit |
| Observability review | djw | TBD | PENDING: no live alert/dashboard evidence |
| Sign-off | djw | TBD | PENDING: human-only |

## Evidence Boundary

No production deployment, real production database operation, staging drill, 24-hour soak, live provider approval, or human launch sign-off occurred during EP-011 through EP-018. The local evidence above may support a later staging exercise; it must not be relabeled as production readiness.

## Post-EP-020 Current Verification (2026-08-11)

EP-019 through EP-028 add verified local evidence only: three fake-Nexus E2E scenarios, governed prebuilt bridge lifecycle tests, structured `/readyz/details`, atomic archive validation, fake-client backup/restore safety tests, Store-backed rate limits, bounded runtime metrics, and owner tooling. EP-029 adds supervised SIGTERM-safe Kernel shutdown, bounded dependency checks, and executor-worker readiness; after Docker-backed services were unavailable, the required smoke, integration, E2E, and full verifier gates passed against isolated loopback Postgres and WSL NATS JetStream services. No production deployment, real production database operation, staging termination-drain, crash-loop drill, 24-hour soak, live provider approval, or human launch sign-off occurred during these plans. The production-readiness gate therefore remains correctly blocked on D1-D5 and the other operator/human-owned evidence listed above.

## Post-EP-030 Current Verification (2026-08-11)

EP-030 adds a durable tenant autonomy-freeze overlay and a confirmation-
gated `hydra-admin` owner operation. Freeze preserves stored cells while
making Governor resolution canonical L1 until thaw, and the existing tenant
policy revision invalidates cached Governors. Store tests cover tenant
isolation, idempotence, thaw restoration, and event emission. This is local
code evidence only; staging D5 execution, live operator evidence, and the
remaining EP-010 launch gates are still required.

## Post-EP-031 Current Verification (2026-08-12)

EP-031 adds an optional profile-gated Prometheus and Alertmanager pair. The
profile scrapes only the Kernel metrics endpoint over an internal network,
loads the existing alert rules, and keeps Alertmanager receiver-neutral with
no default outbound destination. Static policy validation and normalized
Compose configuration pass locally. This does not prove live notification
delivery, dashboards, staging observability response, or any EP-010 launch
gate; those remain operator-owned.

## Post-EP-032 Current Verification (2026-08-12)

EP-032 adds an optional `backup` profile that invokes the existing atomic
Postgres backup helper from a pinned PostgreSQL client image. The scheduler
validates its interval, supports deterministic once-mode tests, exits on
helper failure, and is restricted to `data-internal` with a named local
backup volume. Shell, normalized Compose, operational, security, and full
local gates remain the evidence boundary. Off-box replication, capacity and
retention policy, restore drills, JetStream/vault recovery, and human EP-010
sign-off remain open.

## Post-EP-021 Current Verification (2026-08-11)

EP-021 adds local evidence that the tag release workflow requires BuildKit `provenance: mode=max`, `sbom: true`, digest capture, and signed `actions/attest@v4` publication, plus a nightly wrapper that discovers and passes `c9_soak_10k`. No tag was created, no image or attestation was published, and no registry verification occurred. EP-010 remains partial on staging drills, recovery, soak, real identity/TLS, privacy/security/accessibility/performance/observability review, retention/export scheduling, and human sign-off.

## Post-EP-022 Current Verification (2026-08-11)

EP-022 adds local evidence for an explicit `HYDRA_EGRESS_PROXY_URL` contract, staging/production fail-closed validation, and proxy-aware LLM/OIDC/Fabric/BridgeHost client construction. The static policy gate and local tests pass without contacting an external provider. No staging proxy ACL, DNS/TLS path, IdP JWKS fetch, or external provider call was exercised; EP-010 remains partial.

## Post-EP-023 Current Verification (2026-08-11)

EP-023 adds a Store-owned versioned tenant export and a read-only age-based retention preview, exposed through authenticated local admin routes with session-derived tenancy. Focused isolated Store and Fabric tests prove tenant scoping, soft-delete visibility, deterministic bounds, cross-tenant denial, and non-admin denial. No purge, scheduler, delivery workflow, legal retention policy, staging privacy demonstration, restore drill, or human sign-off occurred; EP-010 remains partial.

## Post-EP-024 Current Verification (2026-08-11)

EP-024 adds additive auth-source and disabled-at columns, marks the historical migration-owned `admin` seed as `development_seed`, and makes both form authentication and existing-session lookup reject that source in every environment. Active operator credentials remain usable. The separate dev bearer fixture remains gated by `HYDRA_ENV=dev`; no owner secret, bootstrap endpoint, staging identity, restore drill, or human sign-off was added. EP-010 remains partial.

## Post-EP-025 Current Verification (2026-08-11)

EP-025 adds Store-owned bounded recovery identities for durable `Approved`
envelopes and a supervised Kernel scan that runs immediately and every second.
The private Executor recovery path reuses the existing tenant-scoped lock,
Governor-approved state, typed handler, verification, receipt, audit, and
outbox behavior without minting a public ExecuteToken. Old `Executing` rows are
reported through `/readyz/details` as `execution_recovery_required` after 15
minutes and are never replayed automatically. Local Store/Kernel tests prove
restart-style recovery and stale-state preservation; staging crash, rollback,
and operator recovery drills remain EP-010 evidence.

## Post-EP-026 Current Verification (2026-08-11)

EP-026 adds migration `0017_rate_limit_windows.sql` and a Store-owned atomic fixed-window repository. Fabric hashes principal/network keys before persistence, all production middleware uses the asynchronous Store authority, and authority or pruning failures return a generic fail-closed 503. Focused Store, Fabric, and Kernel tests pass, including concurrent admission and digest non-disclosure. This is local executable evidence only; multi-replica staging quotas, outage drills, and human production sign-off remain EP-010 gaps.

## Post-EP-027 Current Verification (2026-08-11)

EP-027 enables the existing Kernel metrics registry in the real request path.
The middleware records bounded method, route, status-class, and duration
labels without reading query strings or emitting tenant, identity, token, or
customer values. The focused Kernel metrics suite passed 8 tests and the
full local verifier remains required evidence for this plan. This is
process-local diagnostic instrumentation only; live scrape authentication,
dashboards, alert delivery, staging outage/observability drills, and human
sign-off remain EP-010 gaps.

## Post-EP-028 Current Verification (2026-08-11)

EP-028 adds a local Rust-only hydra-admin owner tool and Store-owned
operator lifecycle repository. Operators are created with auth_source=operator
after Argon2id hashing from stdin, mutations require explicit confirmation,
disabling revokes active sessions, and the historical development_seed cannot
be enabled. Existing external business bindings can be created and moved
between active, disabled, and revoked statuses without deletion. The focused
CLI suite passed 4 tests and the isolated Store suite passed 3 tests, including
real session revocation and seed protection. The full local verifier remains
required evidence for this plan; owner credential custody, staging identity and
TLS, recovery drills, and human sign-off remain EP-010 gaps.

## Post-EP-033 Current Verification (2026-08-12)

EP-033 adds code-level evidence for bounded TOKENKILLER BridgeEngineer mapping
proposals and an authenticated durable A2A proposal workflow. It does not
provide generated code, bridge activation, synchronization, conformance,
canary, promotion, or live-provider evidence. The proposal path is therefore
Experimental and non-executable. EP-010 remains PARTIALLY PASSED pending its
staging, recovery, operational, security, performance, accessibility,
observability, and human sign-off evidence.

## Post-EP-034 Current Verification (2026-08-12)

EP-034 adds migration `0019_tenant_adapter_kv.sql` and routes BridgeHost
lifecycle scratch state through the tenant-scoped Store table. Focused Store,
BridgeHost, and Kernel tests passed against disposable PostgreSQL schemas.
The historical unscoped `adapter_kv` table remains untouched and its legacy
Store methods fail closed. This is local code evidence only; synchronization,
staging drills, and EP-010 production-readiness evidence remain open.

## Post-EP-035 Current Verification (2026-08-12)

EP-035 adds one manually invoked, governed incremental synchronization page.
Store owns tenant/adapter/kind cursor and conflict state; BridgeHost invokes
the existing WIT `changes-since` export through the configured Wasmtime grant;
and the Kernel applies canonical bridge-origin upserts or soft deletes through
the typed handler. Fabric exposes authenticated MCP and REST proposals with
durable idempotency, while no caller can provide tenant or cursor authority.

Store, BridgeHost, Kernel, Fabric, MCP contract, SQLx preparation, and
workspace checks pass against disposable loopback PostgreSQL. No scheduler,
full-relist, synthesis, conformance, canary, promotion, staging deployment,
restore drill, soak, or human production-readiness evidence was performed.
EP-010 remains PARTIALLY PASSED.

## Post-EP-036 Current Verification (2026-08-12)

EP-036 adds a bounded authenticated bridge-conformance read path over the
existing digest-pinned BridgeHost runtime. Focused BridgeHost validator and
fixture tests, Kernel runtime isolation/no-mutation coverage, and Fabric A2A
success/failure/idempotency tests pass against disposable loopback services.
This is code-level evidence only: it does not establish adapter activation,
synchronization scheduling, canary/promotion, provider reliability, staging
deployment, restore/rollback, soak, accessibility, or human sign-off.
EP-010 remains PARTIALLY PASSED.

## Post-EP-037 Current Verification (2026-08-12)

EP-037 closes the code-owned WIT full-relist fallback gap for manual governed
bridge synchronization. It does not close EP-010: no staging provider run,
scheduled-worker drill, restore/rollback exercise, soak, live alert review,
performance/accessibility review, or human production sign-off has occurred.

## Post-EP-038 Current Verification (2026-08-12)

EP-038 makes the in-repository smoke gate hermetic. The real Kernel child now
receives a schema-scoped database URL from `store::TestDb`; that helper owns a
unique schema, runs the embedded migrations, and preserves the existing
Store-backed rate limiter and readiness paths. Child shutdown and schema
cleanup are attempted after endpoint, assertion, setup, and spawn failures.
The focused smoke passed twice against a newly initialized loopback database
without applying root-database migrations first. This is local clean-room
evidence only and does not change EP-010's PARTIALLY PASSED status: staging
drills, recovery/rollback, soak, live identity/TLS, operational reviews,
performance/accessibility evidence, and human sign-off remain open.

## Post-EP-039 Current Verification (2026-08-12)

EP-039 adds additive migration `0022_bridge_sync_schedule.sql`, tenant-scoped
lease claims, owner-confirmed schedule operations, and an opt-in supervised
Kernel worker. The worker creates only the existing governed
`hydra.bridges.sync` envelope with `hydra.scheduler` provenance and stable slot
idempotency; it does not approve, call BridgeHost directly, or mutate CRM data
outside the existing Executor path. Focused Store, Fabric, Kernel, admin, and
full local verifier evidence is required for the final plan gate. This remains
local disposable-service evidence only: EP-010 staging provider runs,
multi-replica scheduling, restore/rollback, soak, live alert review,
performance/accessibility review, and human sign-off remain open.

## Post-EP-040 Current Verification (2026-08-12)

EP-040 adds no schema or external dependency. The existing Kernel metrics
registry is shared by the library scheduler and binary `/metrics` route, and
exposes only bounded scheduler outcome labels. Focused local tests prove two
concurrent workers cannot claim one due slot twice, a proposal survives a
simulated worker interruption and is reused after lease expiry, and stale
completion cannot overwrite the reclaimed lease. The corrected metrics command
discovers nine actual tests rather than silently passing zero binary tests.
This is disposable loopback evidence only; it does not satisfy EP-010
multi-replica staging, durable monitoring, recovery drills, soak, or human
sign-off requirements.

## Post-EP-041 Current Verification (2026-08-12)

EP-041 replaces the previous non-empty launch-table grep with an exact
fail-closed evidence contract. D1-D5 now require exact `PASS` rows, named
operators, non-placeholder evidence, and fresh non-future ISO dates. The
launch table now requires explicit fresh `PASS` rows for every operational and
review gate plus a named `Sign-off` row. Fixture tests prove pending, stale,
malformed, duplicate, future, and placeholder evidence cannot pass. The
checked-in ledger remains intentionally `TBD`/`PENDING`, so the readiness gate
continues to fail on missing D1 evidence. No staging drill, human review, or
production action occurred.

## Post-EP-042 Current Verification (2026-08-12)

EP-042 closes the configuration-owned public metrics exposure. Caddy now
returns `404` for `/metrics*` before its catch-all Kernel proxy, while the
internal Prometheus profile continues to scrape `kernel:8080/metrics` over
`ingress-internal`. Static policy and negative fixture tests pass, but this is
local topology evidence only; it does not establish staged Caddy behavior,
live scrape authentication, dashboards, alert delivery, or human review.
EP-010 remains partial.

## Post-EP-050 Current Verification (2026-08-12)

EP-050 adds a bounded `hydra-kernel --replay-events` recovery command. Store
selects validated unparked canonical outbox rows in stable order with a
maximum batch of 1000; the Kernel requires explicit confirmation, republishes
through the existing acknowledged JetStream publisher, preserves the event
ID/subject/payload/trace carrier, stops on the first failure, and does not
claim or update outbox rows. The disposable integration test runs the real
Kernel child twice, proves one logical durable fake-Nexus event after repeat
delivery, and verifies unchanged Postgres publication fields.

This is local executable evidence only. It does not prove a staged broker-loss
recovery, off-box backup, JetStream snapshot/restore, consumer operational
runbook execution, or human launch sign-off. EP-010 remains partial.

## Post-EP-044 Current Verification (2026-08-12)

EP-044 corrected the configured bridge runtime to use the existing explicit
proxy-aware `ReqwestEgressClient` instead of the deny-only test client. The
bridge lifecycle and runtime-wiring suites passed against disposable loopback
Postgres, and the static egress contract now rejects a regression to the
unconditional deny path. This remains local code evidence; staging proxy
reachability, ACL/DNS/TLS validation, recovery drills, soak, reviews, and
human launch sign-off remain open. EP-010 remains partial.

## Post-EP-043 Current Verification (2026-08-12)

EP-043 reconciles smoke validation with the EP-042 ingress boundary. Public
smoke checks now use `HYDRA_SMOKE_URL` only for health/readiness; metrics are
skipped explicitly unless a distinct `HYDRA_SMOKE_INTERNAL_METRICS_URL` is
provided. Fake-curl fixtures prove public metrics are never requested and
equal public/internal URLs fail closed. This is local validation evidence only
and does not establish a staging smoke run or production readiness. EP-010
remains partial.

## Post-EP-052 Current Verification (2026-08-12)

EP-052 promotes the existing performance checks into one required local gate.
The release-only Governor p99 test passed 1/1, the named `c9_soak_10k` bridge
conformance test passed 1/1, and the TOKENKILLER cache audit remains part of the
same wrapper. `verify.sh` and nightly now use `scripts/test-performance.sh`
without masked failures or duplicate raw ignored-test paths.

This strengthens local regression detection but does not satisfy EP-010's
staging shell/API latency measurements, 24-hour soak, live provider budget and
cache evidence, or human performance review. EP-010 remains partial.

## Post-EP-053 Current Verification (2026-08-12)

EP-053 closes the code-owned Shell accessibility/degradation slice. The Shell
now has a skip link, explicit navigation landmark, native `details`/`summary`
disclosures for New Deal and Kind Overrides, and a dependency-free contract
test that rejects nested forms and missing native POST fallbacks. The required
local gate is included in preflight and `verify.sh`, and the full verifier
exited 0 in 862.8 seconds.

This does not certify browser keyboard traversal, screen-reader behavior,
contrast, staging no-JavaScript behavior, or human accessibility review. Those
remain open EP-010 gates, and Hydra remains not production-ready.

## Post-EP-054 Current Verification (2026-08-12)

EP-054 hardens the release-facing helpers without performing deployment. The
staging helper now requires a Buildx digest and owner-provided known-hosts
record, uses strict SSH host checking, and pulls the exact image digest before
Compose. The promotion helper now fails closed when `curl` or Docker is absent,
requires staging health and readiness, verifies the same digest, reports dry
runs distinctly, and pushes only the immutable production tag.

`bash scripts/test-deployment-safety.sh` and
`bash scripts/check-release-policy.sh` pass locally. These checks do not
provide signed tag-run, registry, staging, SSH, rollback, or human release
evidence; EP-010 remains partial.

## Post-EP-055 Current Verification (2026-08-12)

EP-055 removes the mutable Hydra `latest` alias from the tag-triggered
release workflow. The Buildx step now publishes only the explicit version tag;
digest-bound staging and immutable production promotion remain unchanged.
Local Compose still accepts `HYDRA_TAG` and its development `local` default.

The release policy and deployment safety gates reject a reintroduced
`hydra/kernel:latest` release tag without contacting a registry or deploying.
This is local policy evidence only: signed tag-run, registry, staging, SSH,
rollback, and human release evidence remain open EP-010 gates.

## Post-EP-056 Current Verification (2026-08-13)

EP-056 replaces the Kernel and event-replay CLI's duplicated plain NATS
connection paths with one typed async-NATS transport boundary. Staging and
production configuration now defaults to and enforces credentials-file
authentication plus TLS, rejects credentials embedded in `NATS_URL`, and
supports explicit private CA and mTLS paths. Development and loopback tests
remain compatible with plain NATS.

The focused Kernel and NATS transport tests passed 19/19 and 5/5, the static
policy gate and preflight passed, and the resource-safe full verifier exited 0
after 731.3 seconds with the terminal `verify: ok` signal. This does not prove
a real broker account, certificate chain, remote/shared NATS connection,
credential rotation, staged broker recovery, or human sign-off; EP-010 remains
partial.

## Post-Audit Configuration Verification (2026-08-13)

The reference Compose Kernel environment no longer repeats the NATS
credential/TLS variables. `scripts/check-nats-policy.sh` now rejects duplicate
transport definitions, and the focused policy check, Compose config parse with
disposable local values, preflight, and the full resource-safe verifier all
pass. This is configuration-drift evidence only; it does not replace staging
broker, recovery, or human production-readiness evidence.

## Post-EP-051 Current Verification (2026-08-12)

EP-051 adds a code-level activation safety gate for prebuilt Wasmtime
adapters. After digest-pinned probe, the Kernel runs the existing bounded
read-only BridgeHost conformance contract with the persisted grant and
configuration; only a passing descriptor/schema/list/incremental-read check
can transition the registry to `active`. A probe-only fixture is rejected,
persisted as `failed`, and recorded in tenant-scoped transition history. The
focused Kernel bridge lifecycle suite passed 3/3.

This does not claim generated adapter code, autonomous canary/promotion,
staging provider validation, recovery drills, soak, or human launch sign-off.
EP-010 remains partial.
