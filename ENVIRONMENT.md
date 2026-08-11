# ENVIRONMENT.md

## Required tools & versions
rustup (Rust 1.79+ pinned in rust-toolchain.toml) plus the `wasm32-wasip2` target, cargo components: rustfmt, clippy; cargo-audit ≥0.20, cargo-deny ≥0.14, sqlx-cli ≥0.7 (`cargo install sqlx-cli --no-default-features --features postgres`), wasm-tools ≥1.200, docker + compose v2, jq, curl, ripgrep. Package manager: cargo only.

## Environment variables
| Name | Req | Env | Example | Secret | Description | Validation |
|---|---|---|---|---|---|---|
| DATABASE_URL | yes | all | postgres://hydra:hydra@localhost:5432/hydra | yes(cred) | sqlx conn | preflight: `pg_isready` equivalent via sqlx ping |
| NATS_URL | yes | all | nats://localhost:4222 | conditional | canonical event transport; treat as secret if credentials are embedded | kernel connects and creates or verifies `HYDRA_CRM_EVENTS_V1` |
| HYDRA_VAULT_KEY | yes | all | SET_LOCAL_DEV_VAULT_KEY | YES | startup-required owner credential reserved for encrypted vault integration | kernel refuses boot without it; persisted bridge-secret loading is not yet runtime-wired |
| HYDRA_BIND | no | all | 0.0.0.0:8080 | no | listen addr | parseable SocketAddr |
| HYDRA_BASE_URL | yes | all | https://crm.example.com | no | canonical links and OAuth resource identifier | absolute URI; https in staging/prod |
| DEEPSEEK_API_KEY | opt* | all | sk-... | YES | deepseek provider | *required if routes use deepseek; else STOP per AGENTS §4 |
| ANTHROPIC_API_KEY | opt | all | sk-ant-... | YES | anthropic provider | as above |
| OPENAI_COMPAT_BASE_URL | opt | all | http://llama-server:8080/v1 | no | self-hosted provider | health GET /models |
| OPENAI_COMPAT_MODEL | opt* | all | hydra-local | no | self-hosted provider model identifier | *required with OPENAI_COMPAT_BASE_URL |
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

## Compose-only variables

| Name | Required | Example | Purpose |
|---|---|---|---|
| POSTGRES_PASSWORD | yes | generated URL-safe value | Sets the bundled Postgres credential and is interpolated into the kernel DSN |
| HYDRA_TAG | no | v0.1.0 | Selects the immutable `hydra/kernel` image tag; defaults to `latest` for local use |
| HYDRA_EVENTS_NETWORK | no | hydra-events | Names the attachable internal event-only network for a trusted Nexus consumer |
| CADDY_HTTP_PORT | no | 80 | Host HTTP redirect port; Caddy only |
| CADDY_HTTPS_PORT | no | 443 | Host HTTPS ingress port; Caddy only |

Secrets INSIDE the vault (referenced by name, not env): suitecrm_client_id, suitecrm_client_secret, smtp_password, oauth_provider_signing_key, social_* tokens.

## Local development setup
1. `bash scripts/install.sh` 2. Create ignored `.env` from `.env.example`; replace `POSTGRES_PASSWORD` and `HYDRA_VAULT_KEY` placeholders and keep the local `DATABASE_URL` password aligned. 3. `docker compose -f docker/compose.yaml up -d postgres nats` 4. `bash scripts/db-setup.sh` 5. `cargo run -p hydra-kernel`.

## Test env
Integration tests self-manage schemas on `DATABASE_URL` and require a JetStream-enabled test NATS at `NATS_URL`; the broker may retain the bounded canonical test stream between runs, and tests isolate events/consumers by generated IDs rather than attempting a second overlapping `hydra.crm.>` stream. DeepSeek/Anthropic are local fakes, so no provider keys are needed for `verify.sh`. In `HYDRA_ENV=dev`, the kernel explicitly enables `Authorization: Bearer hydra-dev-admin` for authenticated local API development only; it is disabled in staging and production. All local routes still require a non-nil `x-hydra-tenant`, while Nexus routes ignore that header and derive Hydra tenant authority only from a validated token plus active binding. Nexus contract tests use a deterministic pinned public key and require no live Nexus service. `scripts/cache-hit-audit.sh` falls back to the documented local Postgres example when `DATABASE_URL` is unset.

## Staging / Production
Use the same image and network boundaries with `HYDRA_ENV=staging|prod`, an immutable image tag, real DNS/TLS, owner-controlled runtime secrets, and either a private bundled event network or a private authenticated shared NATS override. The checked-in Caddyfile uses a local CA and is not production TLS configuration. The current runtime validates `HYDRA_VAULT_KEY`, but persisted encrypted-vault/bridge lifecycle wiring remains unavailable and is a production-readiness gap.

## Configuration validation
Kernel boots through `config::validate()` — missing required vars print a single table of failures and exit 78.

## Troubleshooting
sqlx offline errors -> `cargo sqlx prepare --workspace`; adapter component build fails immediately -> `rustup target add wasm32-wasip2`; NATS unavailable -> inspect `docker compose logs --tail=50 postgres nats` and `/readyz` rather than publishing port 4222; Nexus boot failure -> verify issuer/audience, exactly one trust anchor, allowed Origins, and an active binding; never regenerate owner credentials outside dev.
