# ENVIRONMENT.md

## Required tools & versions
rustup (Rust 1.79+ pinned in rust-toolchain.toml) plus the `wasm32-wasip2` target, cargo components: rustfmt, clippy; cargo-audit ≥0.20, cargo-deny ≥0.14, sqlx-cli ≥0.7 (`cargo install sqlx-cli --no-default-features --features postgres`), wasm-tools ≥1.200, docker + compose v2, jq, curl, ripgrep. Package manager: cargo only.

## Environment variables
| Name | Req | Env | Example | Secret | Description | Validation |
|---|---|---|---|---|---|---|
| DATABASE_URL | yes | all | postgres://hydra:hydra@localhost:5432/hydra | yes(cred) | sqlx conn | preflight: `pg_isready` equivalent via sqlx ping |
| NATS_URL | yes | all | nats://localhost:4222 | no | canonical event transport; credentials in the URL are rejected | `nats://` or `tls://` endpoint list; secure environments also require auth and TLS |
| NATS_CREDS_FILE | yes* | all | /run/secrets/nats.creds | YES | mounted NATS user credentials file | *required when `NATS_REQUIRE_AUTH=true`; never put credentials in `NATS_URL` |
| NATS_REQUIRE_AUTH | no | all | false (dev), true (staging/prod) | no | require NATS credentials-file authentication | boolean; cannot be false in staging/prod |
| NATS_TLS_REQUIRED | no | all | false (dev), true (staging/prod) | no | require TLS for the NATS client connection | boolean; cannot be false in staging/prod |
| NATS_TLS_CA_FILE | opt | all | /run/secrets/nats-ca.pem | no | private NATS TLS CA bundle | readable PEM file when set |
| NATS_TLS_CLIENT_CERT_FILE | opt | all | unset | no | mTLS client certificate | readable file; requires `NATS_TLS_CLIENT_KEY_FILE` |
| NATS_TLS_CLIENT_KEY_FILE | opt | all | unset | YES | mTLS client key | readable file; requires `NATS_TLS_CLIENT_CERT_FILE` |
| HYDRA_VAULT_KEY | yes | all | SET_LOCAL_DEV_VAULT_KEY | YES | owner passphrase for the age-encrypted named-secret vault | kernel requires it; staging/prod fail closed if the configured vault cannot decrypt |
| HYDRA_VAULT_PATH | no | all | data/vault.age | no | age-encrypted named-secret vault path | defaults to `data/vault.age`; staging/prod must provide a readable encrypted file |
| HYDRA_VAULT_RESTORE_CONFIRM | no | owner tooling | unset | no | explicit confirmation for `hydra-vault restore` | must equal `restore`; never used by Kernel startup |
| HYDRA_ADAPTERS_PATH | no | all | /app/adapters | no | trusted root for prebuilt Wasmtime bridge components | lifecycle handlers are unavailable unless the path is valid and a usable configured SecretSource is available |
| HYDRA_BRIDGE_SYNC_SCHEDULER_ENABLED | no | all | false | no | explicitly enables the supervised durable bridge-sync proposal scheduler | boolean; default false; enabled mode fails closed unless the governed bridge-sync handler is registered |
| HYDRA_BIND | no | all | 0.0.0.0:8080 | no | listen addr | parseable SocketAddr |
| HYDRA_SHUTDOWN_TIMEOUT_SECONDS | no | all | 30 | no | maximum wait per background task during coordinated shutdown | positive u64; task is aborted and the process fails if it does not stop in time |
| HYDRA_DEPENDENCY_TIMEOUT_SECONDS | no | all | 5 | no | deadline for startup and individual Postgres/NATS/readiness dependency operations | positive u64; timeouts fail closed |
| HYDRA_BASE_URL | yes | all | https://crm.example.com | no | canonical links and OAuth resource identifier | absolute URI; https in staging/prod |
| HYDRA_EGRESS_PROXY_URL | yes* | all | http://egress-proxy:8888 | no | explicit proxy for configured external HTTP clients | *optional in dev/test; required in staging/prod; absolute http(s), no embedded credentials |
| DEEPSEEK_API_KEY | opt* | all | sk-... | YES | deepseek provider | *required if routes use deepseek; else STOP per AGENTS §4 |
| ANTHROPIC_API_KEY | opt | all | sk-ant-... | YES | anthropic provider | as above |
| OPENAI_COMPAT_BASE_URL | opt | all | http://llama-server:8080/v1 | no | self-hosted provider | health GET /models |
| OPENAI_COMPAT_MODEL | opt* | all | hydra-local | no | self-hosted provider model identifier | *required with OPENAI_COMPAT_BASE_URL |
| NEXUS_MODEL_GATEWAY_URL | opt* | Nexus | https://model-gateway.nexus.example/v1 | no | optional OpenAI-compatible Nexus model gateway | required with model and vault token name; absolute URI; https in staging/prod |
| NEXUS_MODEL_GATEWAY_MODEL | opt* | Nexus | nexus-reasoning | no | model identifier sent to the configured Nexus gateway | required with NEXUS_MODEL_GATEWAY_URL |
| NEXUS_MODEL_GATEWAY_TOKEN_SECRET | opt* | Nexus | nexus_model_gateway_token | no | named vault secret for the gateway bearer token | required with NEXUS_MODEL_GATEWAY_URL; raw token never enters env or logs |
| NEXUS_MODEL_GATEWAY_PRIVATE | no | Nexus | false | no | declares whether the gateway is approved for private/PII routes | boolean; false fails PII routing closed |
| HYDRA_SKILLS_PATH | opt* | all | /app/skills | no | owner-controlled signed Agent Skills package directory | required with HYDRA_SKILLS_TRUST_FILE; non-symlink directory |
| HYDRA_SKILLS_TRUST_FILE | opt* | all | /app/skill-trust.json | no | owner-controlled Ed25519 trust-anchor and least-authority policy file | required with HYDRA_SKILLS_PATH; regular file |
| TK_HIT_RATIO_TARGET | no | all | 0.97 | no | ledger SLO | 0<r<1 |
| TK_OUTPUT_BUDGET_BYTES | no | all | 16384 | no | default NukeGuard cap | u32 |
| HYDRA_GOVERNOR_MONTHLY_SPEND_CAP_CENTS | no | all | 50000 | no | deterministic monthly execution cap | positive u64 |
| HYDRA_GOVERNOR_PII_EGRESS_ALLOWLIST | no | all | private | no | provider tags permitted for PII egress | comma-separated non-empty tags |
| HYDRA_GOVERNOR_BLAST_ENTITIES_CEILING | no | all | 250 | no | entity-count ceiling before autonomy clamp | u32 |
| HYDRA_GOVERNOR_BLAST_SENDS_CEILING | no | all | 50 | no | external-send ceiling before autonomy clamp | u32 |
| HYDRA_GOVERNOR_BLAST_MONEY_CEILING_CENTS | no | all | 250000 | no | monetary ceiling before autonomy clamp | u64 |
| RUST_LOG | no | dev | info,hydra=debug | no | tracing filter | — |
| HYDRA_ENV | yes | all | dev\|staging\|prod | no | env gates | enum |
| NEXUS_INTEGRATION_ENABLED | no | all | false | no | enables authenticated Nexus REST/MCP boundary | boolean; default false |
| NEXUS_OIDC_ISSUER | yes* | Nexus | https://id.nexus.example | no | trusted asymmetric token issuer | *required when enabled; absolute URI; https in staging/prod |
| NEXUS_OIDC_AUDIENCE | yes* | Nexus | hydra-api | no | required resource/audience claim | *required when enabled |
| NEXUS_OIDC_JWKS_URL | opt* | Nexus | https://id.nexus.example/.well-known/jwks.json | no | cached online trust anchor | exactly one JWKS URL or public-key file when enabled |
| NEXUS_OIDC_PUBLIC_KEY_FILE | opt* | Nexus | /run/secrets/nexus-public.pem | no | offline/private deployment trust anchor | readable public-key file; never a private key |
| NEXUS_OIDC_ALLOWED_ALGORITHMS | no | Nexus | RS256 | no | asymmetric JWT allowlist | comma-separated supported asymmetric algorithms |
| NEXUS_ALLOWED_MCP_ORIGINS | yes* | Nexus | https://nexus.example | no | MCP Origin allowlist | *at least one absolute URI when enabled |
| NEXUS_JWKS_CACHE_SECONDS | no | Nexus | 300 | no | verified JWKS cache lifetime | positive u64 |
| NEXUS_OIDC_CLOCK_SKEW_SECONDS | no | Nexus | 30 | no | JWT time-claim leeway | u64 |
| NEXUS_MCP_MAX_REQUEST_BYTES | no | Nexus | 1048576 | no | Streamable HTTP body ceiling | positive integer |
| NEXUS_APPROVAL_AUTH_STRENGTHS | no | Nexus | mfa | no | accepted OIDC `acr` values for external approvals | comma-separated non-empty values |
| HYDRA_BACKUP_DIR | no | ops | ./backups | no | directory for timestamped PostgreSQL backup archives | created if absent; archives are private and validated before publication |
| HYDRA_RESTORE_CONFIRM | no | ops/test | ephemeral | no | explicit acknowledgement for disposable restore verification only | `scripts/db-restore.sh` requires the exact value `ephemeral` and refuses `HYDRA_ENV=prod|production` |
| HYDRA_ADMIN_CONFIRM | no | owner ops | I_UNDERSTAND | no | explicit acknowledgement for owner-only user/binding mutations | `hydra-admin` requires the exact value `I_UNDERSTAND`; read/list commands do not |
| HYDRA_SMOKE_URL | no | smoke/staging ops | https://hydra.example.com | no | public ingress URL for `/healthz` and `/readyz` smoke checks | must not be used for metrics; public Caddy denies `/metrics*` |
| HYDRA_SMOKE_INTERNAL_METRICS_URL | no | smoke/staging ops | http://kernel:8080/metrics | no | optional separately reachable internal metrics URL for public smoke validation | must differ from `HYDRA_SMOKE_URL`; metrics series are checked only when supplied |

## Compose-only variables

| Name | Required | Example | Purpose |
|---|---|---|---|
| POSTGRES_PASSWORD | yes | generated URL-safe value | Sets the bundled Postgres credential and is interpolated into the kernel DSN |
| HYDRA_TAG | no | v0.1.0 | Selects the immutable `hydra/kernel` and `hydra/egress-proxy` image tags; defaults to `latest` for local use |
| HYDRA_EVENTS_NETWORK | no | hydra-events | Names the attachable internal event-only network for a trusted Nexus consumer |
| CADDY_HTTP_PORT | no | 80 | Host HTTP redirect port; Caddy only |
| CADDY_HTTPS_PORT | no | 443 | Host HTTPS ingress port; Caddy only |

Secrets INSIDE the vault (referenced by name, not env): suitecrm_client_id, suitecrm_client_secret, smtp_password, oauth_provider_signing_key, social_* tokens. Provision them with `hydra-vault set <name>` from stdin; `get-names` prints names only. `rotate` decrypts with `HYDRA_VAULT_KEY` and replaces the file using `HYDRA_VAULT_NEXT_KEY`. `backup <destination>` validates and copies ciphertext without replacing an existing destination. `restore <source>` requires `HYDRA_VAULT_RESTORE_CONFIRM=restore` and validates before atomically replacing the active artifact.

## Local development setup
1. `bash scripts/install.sh` 2. Create ignored `.env` from `.env.example`; replace `POSTGRES_PASSWORD` and `HYDRA_VAULT_KEY` placeholders and keep the local `DATABASE_URL` password aligned. 3. Optionally provision `data/vault.age` with `cargo run -p hydra-vault -- set <name>`. 4. `docker compose -f docker/compose.yaml up -d postgres nats` 5. `bash scripts/db-setup.sh` 6. `cargo run -p hydra-kernel`. A missing development vault disables bridge secrets but does not create a production-like fallback.

## Test env
Integration tests self-manage schemas on `HYDRA_TEST_DATABASE_URL` (falling back to `DATABASE_URL` for compatibility) and fail closed unless the URL targets loopback Postgres; they require a JetStream-enabled test NATS at `NATS_URL`. The broker may retain the bounded canonical test stream between runs, and tests isolate events/consumers by generated IDs rather than attempting a second overlapping `hydra.crm.>` stream. DeepSeek/Anthropic are local fakes, so no provider keys are needed for `verify.sh`. `production-readiness-check.sh` is stricter: it requires explicitly supplied `HYDRA_TEST_DATABASE_URL` and `NATS_URL` values, rejects inherited or non-loopback database and broker targets, rejects NATS URL credentials, and exports the database URL as `DATABASE_URL` for every nested gate. In `HYDRA_ENV=dev`, the kernel explicitly enables `Authorization: Bearer hydra-dev-admin` for authenticated local API development only; it is disabled in staging and production. Local REST sessions derive tenant authority from the verified session, while the legacy `x-hydra-tenant` header is accepted only when it matches that session (the development identity path remains test-only); Nexus routes ignore that header and derive Hydra tenant authority only from a validated token plus active binding. Nexus contract tests use a deterministic pinned public key and require no live Nexus service. `scripts/cache-hit-audit.sh` falls back to the documented local Postgres example when `DATABASE_URL` is unset. In staging and production, `HYDRA_EGRESS_PROXY_URL` is mandatory and is passed explicitly to configured model-provider and OIDC JWKS clients; generic `HTTP_PROXY`/`HTTPS_PROXY` variables are not a substitute. Database, NATS, ingress, and local health traffic do not use this external proxy.
The historical database seed row is disabled by migration `0016_auth_seed_hardening.sql` and is rejected by form login and session lookup in dev, staging, and production. Provisioning an active operator and Nexus business binding is owner-controlled through the local `hydra-admin` binary; it never runs migrations, accepts passwords from arguments, or provides a public bootstrap endpoint.

The real Kernel rate limiter uses the Store/Postgres authority and does not fall back to process-local state when that authority is unavailable. Local Fabric test applications may use the explicit in-memory constructor; this is not a production deployment profile. Rate-limit rows contain only versioned SHA-256 digests and are operational state, not tenant or authorization data.

## Staging / Production

Bridge scratch state is stored in the additive `tenant_adapter_kv` table. Do
not backfill or reuse historical `adapter_kv` rows because they contain no
tenant authority.
Use the same image and network boundaries with `HYDRA_ENV=staging|prod`, an immutable image tag, real DNS/TLS, owner-controlled runtime secrets, and either a private bundled event network or a private authenticated shared NATS override. The checked-in Caddyfile uses a local CA and is not production TLS configuration. The runtime loads `HYDRA_VAULT_PATH` through the age-backed BridgeHost source and fails closed when it is missing or invalid. Set `HYDRA_ADAPTERS_PATH` to the immutable prebuilt component root only when governed deploy/pause/resume/manual synchronization is intentionally enabled. Bounded full-relist synchronization is supported for adapters declaring `incremental_sync: false`; owner-controlled scheduling is optional and disabled by default through `HYDRA_BRIDGE_SYNC_SCHEDULER_ENABLED`. Mapping synthesis remains experimental when a configured TOKENKILLER route is available; generated adapters, autonomous canary, and promotion remain unavailable.

## Configuration validation
Kernel boots through `config::validate()` — missing required vars print a single table of failures and exit 78. `HYDRA_DEPENDENCY_TIMEOUT_SECONDS` bounds startup and readiness dependency calls; `HYDRA_SHUTDOWN_TIMEOUT_SECONDS` bounds each owned relay/executor shutdown join. Both reject zero and malformed values.

## Troubleshooting
sqlx offline errors -> `cargo sqlx prepare --workspace`; adapter component build fails immediately -> `rustup target add wasm32-wasip2`; NATS unavailable -> inspect `docker compose logs --tail=50 postgres nats` and `/readyz` rather than publishing port 4222; Nexus boot failure -> verify issuer/audience, exactly one trust anchor, allowed Origins, and an active binding; never regenerate owner credentials outside dev.
