# EP-056 NATS Transport Authentication
Plan status: COMPLETE

## 1. Purpose / Big Picture
Close the remaining code-owned broker security gap between Hydra and NATS.
The current Kernel connects with plain `async_nats::connect` and the reference
staging contract has no enforced credentials or TLS. This plan adds one typed
connection boundary, fail-closed staging/production validation, and mandatory
static/runtime tests while retaining local development compatibility.

## 2. Scope
Configuration parsing, async-NATS connection construction, event replay reuse,
Compose environment wiring, policy gates, documentation, and additive tests.

## 3. Non-goals
No NATS server deployment or account creation, certificate generation, secret
material, database migration, public broker exposure, production connection,
registry action, or unrelated authentication refactor.

## 4. Context and Orientation
EP-055 is complete. `crates/kernel/src/main.rs::connect_nats` currently calls
`async_nats::connect(&config.nats_url)` and the replay CLI does the same. The
existing async-nats 0.49.1 dependency exposes verified `ConnectOptions` methods
for credentials files, root certificates, client certificates, and required
TLS. `HYDRA_ENV` already distinguishes dev, staging, and prod. The checked-in
Compose network keeps NATS private, but the package contract says a shared or
remote broker needs authentication and operator-owned trust.

## 5. Files to Read First
`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`;
`.agent/EXECUTION_RULES.md`; `SPEC-036-nats-transport-auth.md`;
`crates/kernel/src/config.rs`; `crates/kernel/src/main.rs`;
`crates/kernel/src/lib.rs`; `Cargo.toml`; `docker/compose.yaml`;
`docker/nexus.env.example`; `.env.example`; `ENVIRONMENT.md`;
`DEPLOYMENT.md`; `SECURITY.md`; `TESTING.md`; `PRODUCTION_READINESS.md`;
`scripts/preflight.sh`; `scripts/verify.sh`; `scripts/check-execplan-state.sh`.

## 6. Files to Change
`.agent/specs/SPEC-036-nats-transport-auth.md`;
`.agent/execplans/EP-056-nats-transport-auth.md`;
`.agent/state/execplan-index.md`; `scripts/check-execplan-state.sh`;
`scripts/check-nats-policy.sh`; `scripts/preflight.sh`; `scripts/verify.sh`;
`crates/kernel/src/lib.rs`; `crates/kernel/src/nats.rs`;
`crates/kernel/src/config.rs`; `crates/kernel/src/main.rs`;
`crates/kernel/tests/nats_transport.rs`; `docker/compose.yaml`;
`docker/nexus.env.example`; `.env.example`; `ENVIRONMENT.md`;
`DEPLOYMENT.md`; `SECURITY.md`; `TESTING.md`; `PRODUCTION_READINESS.md`;
`DECISIONS.md`; `COMMANDS.md`; `Cargo.toml`; `Cargo.lock`.

## 7. Interfaces and Contracts
Add a public Kernel library `NatsTransportConfig` plus one async connect
function that applies credentials-file, CA, client-certificate, client-key,
and `require_tls` options without exposing secret contents. `Config` owns a
validated instance. The regular Kernel and `--replay-events` use it. Existing
plain `nats://` loopback behavior remains valid only for dev/test. Secure
environments require `NATS_CREDS_FILE`, `NATS_REQUIRE_AUTH=true`, and
`NATS_TLS_REQUIRED=true`; URL userinfo is never accepted.

## 8. Milestones
M1 - Add SPEC-036, EP-056, active index state, and checker coverage. Validation:
`bash scripts/check-execplan-state.sh` -> `execplan state: ok`.

M2 - Add typed NATS options, secure environment validation, and reuse the
connection boundary in Kernel startup and event replay. Validation:
`cargo test -p hydra-kernel --lib --offline` and the focused NATS test pass.

M3 - Wire Compose/examples/docs and mandatory `check-nats-policy.sh` gate.
Validation: `bash scripts/check-nats-policy.sh`, `bash scripts/preflight.sh`,
`cargo fmt --all -- --check`, and `git diff --check` pass.

M4 - Run the resource-safe full verifier and reconcile the plan. Validation:
`bash scripts/verify.sh` -> `verify: ok`; state checker -> `execplan state: ok`.

## 9. Concrete Steps
1. Add secure transport fields and validation without storing secret contents.
2. Use verified async-nats `ConnectOptions` methods for all production paths.
3. Reject incomplete secure configurations and embedded URL credentials.
4. Keep dev/test defaults plain and explicit in examples.
5. Add focused tests for secure defaults, credential/TLS pairing, URL hygiene,
   custom CA/client certificates, and redacted errors.
6. Make the policy check mandatory in preflight and full verification.
7. Record ADR-0067 and the local-only evidence; preserve EP-010 gaps.

## 10. Validation and Acceptance
Run in order:
1. `bash scripts/check-execplan-state.sh`
2. `cargo test -p hydra-kernel --lib --offline`
3. `cargo test -p hydra-kernel --test nats_transport --offline -- --nocapture`
4. `bash scripts/check-nats-policy.sh`
5. `bash scripts/preflight.sh`
6. `cargo fmt --all -- --check`
7. `git diff --check`
8. Resource-safe `bash scripts/verify.sh` with the documented loopback
   Postgres/NATS environment.

Expected markers are `execplan state: ok`, focused Rust success, `nats policy:
ok`, `preflight: ok`, and terminal `verify: ok`. No external broker or
production action is part of acceptance.

## 11. Idempotence and Recovery
All configuration and documentation changes are repeatable. If a focused test
fails, inspect the exact error and narrow to the relevant config or connection
test before changing code. If secure credentials or certificates are absent,
keep the secure staging/prod path fail closed and validate the dev/test path;
do not invent or commit secret material.

## 12. Progress
- [x] M1 - SPEC, plan, state, and checker activation
- [x] M2 - Typed secure NATS connection boundary and tests
- [x] M3 - Compose, docs, and mandatory policy gate
- [x] M4 - Full verification and truthful completion

## 13. Surprises & Discoveries
- 2026-08-12 - The installed async-nats 0.49.1 dependency already provides
  credentials-file, root-certificate, client-certificate, and required-TLS
  builders, so no new dependency is needed.
- 2026-08-12 - The normal Kernel and event-replay CLI had separate plain NATS
  connection paths; both must use the same boundary.
- 2026-08-12 - The workspace disabled async-nats's `nkeys` feature, so the
  existing credentials-file API was not compiled; enabling that existing
  feature is required and does not add a new direct dependency.

## 14. Decision Log
- 2026-08-12 - Use a mounted NATS credentials file rather than username and
  password in `NATS_URL`; this avoids secret-bearing URLs and reuses the
  existing async-nats nkeys feature.
- 2026-08-12 - Require auth and TLS by default in staging/prod while leaving
  dev/test plain loopback compatible; this is the smallest boundary that
  reconciles the existing deployment contract without changing local services.
- 2026-08-12 - Enable async-nats `nkeys` as an existing dependency feature so
  mounted credentials files are parsed by the verified client implementation;
  cargo audit/deny remain required before completion.
- 2026-08-13 - The resource-safe full verifier completed successfully against
  disposable loopback services, including the NATS policy gate and terminal
  `verify: ok` result; no production broker or database was used.

## 15. Outcomes & Retrospective
M1-M4 are complete. The typed boundary is used by normal Kernel startup and
event replay; secure configuration rejects incomplete staging/prod auth/TLS;
plain dev compatibility is covered by focused tests; Compose/docs and the
mandatory policy gate are aligned. The resource-safe full verifier exited 0
after 731.3 seconds with `preflight: ok`, `nats policy: ok`, `lint: ok`,
`format check: ok`, and the repository's terminal `verify: ok` signal. Staging
broker authentication, trust-anchor custody, and production evidence remain
operator-owned EP-010 gates.
