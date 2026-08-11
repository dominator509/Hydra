# DEPLOYMENT.md

## Status

Hydra has a locally verifiable container package for standalone and Nexus-connected modes. This document is an operator contract, not evidence that staging or production deployment, restore, rollback, soak, or human sign-off has occurred. Production deployment requires separate explicit authorization.

## Topology

```text
public/Nexus HTTPS -> Caddy -> ingress-internal -> kernel
kernel -> data-internal -> Postgres
kernel -> events-internal -> NATS JetStream <- trusted Nexus consumer
kernel -> proxy-internal -> egress-proxy -> egress-external
```

Only Caddy publishes host ports by default. Kernel, Postgres, NATS client, and NATS monitoring ports are not host-published. Postgres and JetStream use different internal networks, so a Nexus consumer attached to `hydra-events` gains no database path. Kernel joins only internal networks; Caddy and the egress proxy are the only dual-homed services.

The base Caddyfile uses an internal CA for local/reference use. A staging or production installer must provide a real DNS/TLS Caddyfile or mounted certificate policy. The kernel must never be placed directly on `ingress-external` or `egress-external`.

## Modes

Standalone is the default: `NEXUS_INTEGRATION_ENABLED=false`, no Nexus trust anchor is required, and external resource-server routes remain disabled.

Nexus-connected mode uses `docker/nexus.env.example` as the configuration template. It enables asymmetric issuer/audience validation, MCP Origin policy, binding-backed tenant resolution, `/v1/nexus/`, MCP `2025-11-25`, and required event readiness. Replace every placeholder before boot. A disabled or missing business binding fails closed while standalone Hydra remains independently usable.

For a Nexus consumer on the same Docker host, attach only that consumer to the named internal `hydra-events` network. For a private shared NATS service, use an operator-owned Compose override that changes the kernel `NATS_URL` and attaches the kernel only to the private events network. Never attach Nexus to `data-internal` and never publish NATS publicly as a shortcut.

## Image

`docker/Dockerfile` builds `hydra/kernel:<version>` from Rust 1.96.1, installs the pinned `wasm32-wasip2` target and `wasm-tools` 1.240.0, compiles the Kernel and memcrm adapter, and runs as non-root on Debian Bookworm slim. Migrations, WIT, and the built adapter are included. Static shell content is compiled into the binary.

The repository does not define an SBOM or release-attestation policy. BuildKit may emit default build metadata, but that is not treated as a reviewed release provenance artifact; SBOM and signed provenance remain release-hardening work.

## Validation

Run the two commands recorded in `COMMANDS.md`:

```bash
docker build -f docker/Dockerfile -t hydra/kernel:local .
docker compose --env-file docker/nexus.env.example -f docker/compose.yaml config
```

The normalized configuration must show host `ports` only on Caddy, separate internal data/events/proxy/ingress networks, and external networks only on Caddy and the egress proxy.

## Migration And Start

1. Pin `HYDRA_TAG` to an immutable tested version and populate owner-controlled secrets.
2. Run `bash scripts/db-backup.sh`; require `backup: ok` before an upgrade.
3. Run the one-shot additive migration: `docker compose --profile ops run --rm migrate`.
4. Start or replace services with the same Compose project.
5. Require `/healthz` and `/readyz` success through Caddy, then run `bash scripts/smoke-test.sh`.

Migrations are forward-only and run before the new kernel serves traffic. Destructive schema or data operations are STOP conditions. The migration service joins only `data-internal` and cannot reach NATS or external networks.

## Release And Rollback

The tag-triggered GitHub release workflow verifies, builds, pushes an image, and may invoke the self-hosted staging deployment job for the repository owner. It contains no production deployment job. Agents must not create a tag or invoke deployment without explicit authorization.

Rollback uses the prior immutable image while retaining additive schema. Follow `ROLLBACK.md`; validate the exact old-image/new-schema combination in staging first. A database restore is a separate owner-controlled recovery operation, never an implicit application rollback.

## Persistence And Recovery

Authoritative CRM, audit, outbox, bindings, idempotency, and approval state live in `hydra-postgres-data`. JetStream state lives in `hydra-nats-data` and supports replay but is not the source of truth. Caddy state uses `caddy-data` and `caddy-config`; `/app/data` is reserved application-local state and must not be treated as the CRM database.

The current repository has a Postgres backup script but no proven JetStream snapshot/restore drill or automated encrypted-vault restore. These remain EP-010 production-readiness gaps. See `NEXUS_PACKAGE_CONTRACT.md` for the future installer inputs and `PRODUCTION_READINESS.md` for the evidence boundary.
