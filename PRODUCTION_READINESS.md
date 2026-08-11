# PRODUCTION_READINESS.md

## Current Status

**NOT PRODUCTION READY. EP-010 remains PARTIAL.** Hydra is production-ready only when `bash scripts/production-readiness-check.sh` emits the exact terminal signal `production-readiness:ok`, every required staging drill and review has dated evidence, and the human launch sign-off is complete. Nexus interoperability passing locally does not satisfy those gates.

## Verified Local Evidence (2026-08-11)

| Gate | Current evidence | Scope |
|---|---|---|
| Preflight | `preflight: ok` | Local repository/tooling |
| Unit | `unit tests: ok` | Workspace unit tests |
| Integration | `integration tests: ok`; `failure suites: ok` | Isolated local Postgres and JetStream |
| Nexus E2E | Two named tests passed; `e2e tests: ok` | Deterministic fake issuer/JWKS and fake Nexus only |
| Security | `security check: ok` | Local source/dependency scan; two allowed advisory warnings remain |
| Dependency policy | `advisories ok, bans ok, licenses ok, sources ok`; `dependency audit: ok` | Current Cargo.lock and deny policy |
| Container image | `hydra/kernel:local` built; adapter emitted `adapters: ok`; non-root image layout passed | Local Docker Desktop only |
| Compose | Standalone and Nexus-connected config exited 0; only Caddy publishes ports | Configuration validation only; no staging deployment |
| Full verifier | Exit 0 in 308.9 seconds through the unconditional terminal `verify: ok` path | Complete local verifier only |

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
| Privacy/data | Export and retention/purge implementation plus staging demonstration; no cron service currently exists |
| Performance | Measured shell/API latency, Governor p99, 10k import, and sustained agent budget/cache results |
| Accessibility | Keyboard, labels/landmarks, and no-JavaScript core-flow review |
| Observability | Live dashboards, alert delivery, event-infrastructure outage response, and redacted-log inspection |
| Recovery | Proven Postgres restore, JetStream recovery, encrypted-secret recovery, and old-image/new-schema rollback |
| Release | SBOM, signed provenance policy, immutable image/version ownership, supported upgrade evidence |
| Human control | Named security/privacy/accessibility/operations owners and final launch sign-off |

## Known Runtime And Packaging Gaps

- `HYDRA_VAULT_KEY` is validated at boot, but a persisted encrypted vault and production BridgeHost secret source are not runtime-wired. Bridge lifecycle capabilities remain unavailable.
- The checked-in Caddyfile uses an internal CA and is local/reference configuration, not production TLS.
- The reference NATS network is private and not host-published, but a remote/shared deployment still needs operator-owned private networking and broker authentication.
- External binding bootstrap has no public endpoint or installer CLI. It remains an owner-operated Hydra provisioning prerequisite; direct SQL is forbidden.
- The rate limiter is process-local fixed-window state, not a distributed multi-replica quota service.
- Session cookies are HttpOnly and SameSite=Lax in current code but do not yet set `Secure`; production session hardening remains open.
- The repository has no implemented cron service for retention, rollups, or scheduled backups and no proven JetStream/vault restore path.
- The repository has no defined SBOM or signed release-provenance policy.

## Supply-Chain Residuals

`scripts/security-check.sh` currently allows `RUSTSEC-2025-0057` (`fxhash`, unmaintained) and `RUSTSEC-2026-0221` (`event-listener`, unsound) under repository policy. `cargo deny` also reports warning-only duplicate versions and unused license allowances. The gates pass, but production review must explicitly accept or remove these residuals.

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
| Privacy/data review | djw | TBD | PENDING: retention/export evidence absent |
| Accessibility review | djw | TBD | PENDING: no executed audit |
| Observability review | djw | TBD | PENDING: no live alert/dashboard evidence |
| Sign-off | djw | TBD | PENDING: human-only |

## Evidence Boundary

No production deployment, real production database operation, staging drill, 24-hour soak, live provider approval, or human launch sign-off occurred during EP-011 through EP-015. The local evidence above may support a later staging exercise; it must not be relabeled as production readiness.
