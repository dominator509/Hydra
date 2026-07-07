# ENVIRONMENT.md

## Required tools & versions
rustup (Rust 1.79+ pinned in rust-toolchain.toml), cargo components: rustfmt, clippy; cargo-audit ≥0.20, cargo-deny ≥0.14, sqlx-cli ≥0.7 (`cargo install sqlx-cli --no-default-features --features postgres`), wasm-tools ≥1.200, docker + compose v2, jq, curl, ripgrep. Package manager: cargo only.

## Environment variables
| Name | Req | Env | Example | Secret | Description | Validation |
|---|---|---|---|---|---|---|
| DATABASE_URL | yes | all | postgres://hydra:hydra@localhost:5432/hydra | yes(cred) | sqlx conn | preflight: `pg_isready` equivalent via sqlx ping |
| NATS_URL | yes | all | nats://localhost:4222 | no | event spine | kernel boot ping |
| HYDRA_VAULT_KEY | yes | all | AGE-SECRET-KEY-1... | YES | decrypts vault/secrets.age | kernel refuses boot w/o |
| HYDRA_BIND | no | all | 0.0.0.0:8080 | no | listen addr | parseable SocketAddr |
| HYDRA_BASE_URL | yes | stage/prod | https://crm.example.com | no | OAuth redirects, links | must be https in prod |
| DEEPSEEK_API_KEY | opt* | all | sk-... | YES | deepseek provider | *required if routes use deepseek; else STOP per AGENTS §4 |
| ANTHROPIC_API_KEY | opt | all | sk-ant-... | YES | anthropic provider | as above |
| OPENAI_COMPAT_BASE_URL | opt | all | http://llama-server:8080/v1 | no | self-hosted provider | health GET /models |
| TK_HIT_RATIO_TARGET | no | all | 0.97 | no | ledger SLO | 0<r<1 |
| TK_OUTPUT_BUDGET_BYTES | no | all | 16384 | no | default NukeGuard cap | u32 |
| RUST_LOG | no | dev | info,hydra=debug | no | tracing filter | — |
| HYDRA_ENV | yes | all | dev\|staging\|prod | no | env gates | enum |

Secrets INSIDE the vault (referenced by name, not env): suitecrm_client_id, suitecrm_client_secret, smtp_password, oauth_provider_signing_key, social_* tokens.

## Local development setup
1. `bash scripts/install.sh` 2. `cp .env.example .env` and fill non-secret vars; generate vault: `age-keygen` → set HYDRA_VAULT_KEY; `hydra vault set <name>` (EP-006 CLI). 3. `docker compose -f docker/compose.yaml up -d postgres nats` 4. `bash scripts/db-setup.sh` 5. `cargo run -p hydra-kernel`.

## Test env
Integration tests self-manage schemas on DATABASE_URL; DeepSeek/Anthropic are wiremock fakes — no keys needed for `verify.sh`.

## Staging / Production
Same compose file + Caddyfile; HYDRA_ENV=staging|prod; real DNS + TLS via Caddy; secrets provisioned by operator into vault before first boot. Parity rule: only env vars in the table may differ across envs.

## Configuration validation
Kernel boots through `config::validate()` — missing required vars print a single table of failures and exit 78.

## Troubleshooting
sqlx offline errors → `cargo sqlx prepare --workspace`; NATS refuse → check compose ports 4222; vault decrypt fail → key mismatch, regenerate only in dev.
