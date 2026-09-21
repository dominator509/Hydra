# DEPLOYMENT.md

## Status

Hydra has a locally verifiable container package for standalone and Nexus-connected modes. This document is an operator contract, not evidence that staging or production deployment, restore, rollback, soak, or human sign-off has occurred. Production deployment requires separate explicit authorization.

## Topology

```text
public/Nexus HTTPS -> Caddy -> ingress-internal -> kernel
kernel -> data-internal -> Postgres
kernel -> events-internal -> NATS JetStream <- trusted Nexus consumer
kernel -> proxy-internal -> egress-proxy -> egress-external
kernel -> ingress-internal -> Prometheus -> observability-internal -> Alertmanager (optional profile)
postgres <- data-internal <- backup-scheduler (optional profile)
hydra-kernel-data -> vault-backup-scheduler -> hydra-vault-backups (optional, network:none profile)
```

Only Caddy publishes host ports by default. Caddy explicitly returns `404`
for `/metrics*`; the process-local metrics surface is reachable only through
the internal `kernel:8080/metrics` path used by Prometheus. Kernel, Postgres, NATS client,
NATS monitoring, Prometheus, and Alertmanager ports are not host-published.
Postgres and JetStream use different internal networks, so a Nexus consumer
attached to `hydra-events` gains no database path. Kernel joins only internal
networks; Caddy and the egress proxy are the only dual-homed services. The
optional observability profile adds a dedicated internal network and does not
change the CRM, NATS, or egress boundaries.

The base Caddyfile uses an internal CA for local/reference use. A staging or production installer must provide a real DNS/TLS Caddyfile or mounted certificate policy; Compose passes `HYDRA_ENV` to Caddy and refuses to start the reference `tls internal` configuration outside development. The kernel must never be placed directly on `ingress-external` or `egress-external`.

## Outbound Egress Contract

Set `HYDRA_EGRESS_PROXY_URL` to the internal Tinyproxy authority (for the reference Compose profile, `http://egress-proxy:8888`). It is optional for development/test, but required before a staging or production Kernel starts and must be an absolute `http` or `https` URI without embedded credentials. Compose admits the Kernel only after the proxy has installed Tinyproxy and its listener healthcheck succeeds; a failed or partial proxy startup therefore fails the dependency boundary closed. The Kernel passes this value explicitly to configured LLM providers and OIDC JWKS retrieval; generic `HTTP_PROXY` and `HTTPS_PROXY` variables do not replace the Hydra configuration check. No database, NATS, ingress, or local health request is routed through this external proxy.

The repository proves client construction and configuration fail-closed behavior locally. Configured OIDC JWKS requests use a bounded five-second deadline; LLM, BridgeHost, and Fabric egress requests use a bounded 30-second request deadline. It does not prove staging DNS/TLS, Tinyproxy ACLs, IdP reachability, or external provider connectivity; those remain operator-owned EP-010 evidence.

## NATS Transport Security

Development and loopback tests may use plain `nats://` without credentials.
Staging and production require `NATS_REQUIRE_AUTH=true` and
`NATS_TLS_REQUIRED=true`; the Kernel rejects either being disabled and refuses
credentials embedded in `NATS_URL`. Mount a NATS credentials file at
`NATS_CREDS_FILE`, and provide `NATS_TLS_CA_FILE` plus the optional mTLS
certificate/key pair when the broker uses a private trust anchor. The event
replay CLI uses the same options. The checked-in Nexus example names the
operator-mounted paths but does not create or mount secret material.

`bash scripts/check-nats-policy.sh` validates this local contract. A real
broker account, certificate chain, remote/shared NATS connectivity, and staged
rotation/recovery evidence remain operator-owned EP-010 gates.

When `HYDRA_ADAPTERS_PATH` is configured, the Kernel constructs the bridge
egress client from `HYDRA_EGRESS_PROXY_URL` before registering lifecycle
handlers. A malformed proxy makes the configured lifecycle unavailable; no
unproxied adapter fallback is allowed. This is locally tested runtime wiring,
not evidence of a reachable staging proxy or upstream CRM.

## Modes

Standalone is the default: `NEXUS_INTEGRATION_ENABLED=false`, no Nexus trust anchor is required, and external resource-server routes remain disabled.

Nexus-connected mode uses `docker/nexus.env.example` as the configuration template. It enables asymmetric issuer/audience validation, MCP Origin policy, binding-backed tenant resolution, `/v1/nexus/`, MCP `2025-11-25`, and required event readiness. The base Compose file's bundled NATS is a plain development broker and is not a Nexus-connected deployment: before boot, an operator must replace the example `NATS_URL` with an authenticated TLS broker URL and provide a Compose override or private shared broker that supplies the mounted credentials and trust anchor named by the example. Replacing placeholders in the example alone is not evidence of broker connectivity. A disabled or missing business binding fails closed while standalone Hydra remains independently usable. The image contains prebuilt adapters at `/app/adapters`; setting `HYDRA_ADAPTERS_PATH=/app/adapters` additionally enables only the governed deploy/pause/resume lifecycle, subject to the configured vault. No bridge sync or synthesis worker is implied.

For a Nexus consumer on the same Docker host, attach only that consumer to the named internal `hydra-events` network. For a private shared NATS service, use an operator-owned Compose override that changes the kernel `NATS_URL` and attaches the kernel only to the private events network. Never attach Nexus to `data-internal` and never publish NATS publicly as a shortcut.

## Observability Profile

The repository includes a profile-gated Prometheus and Alertmanager pair:

```text
docker compose --profile observability --env-file docker/nexus.env.example \
  -f docker/compose.yaml config
```

Prometheus scrapes only `kernel:8080/metrics` and loads the committed
`docker/alerts.yaml` rules. Alertmanager receives and groups those alerts on
`observability-internal`, but its checked-in `hydra-operator` receiver has no
outbound destination. A staging or production operator must supply a
separately reviewed Compose override for an approved notification endpoint;
the repository does not claim live alert delivery or expose monitoring ports
publicly by default. Caddy's public catch-all deliberately does not proxy
`/metrics*`; run `bash scripts/check-ingress-policy.sh` and
`bash scripts/check-observability.sh` before using the profile.

## Backup Profile

The optional `backup` profile invokes the existing atomic, archive-validating
Postgres helper from a pinned `postgres:16-alpine` client container:

```text
docker compose --profile backup --env-file docker/nexus.env.example \
  -f docker/compose.yaml config
```

`backup-scheduler` writes timestamped archives to the named `hydra-backups`
volume once per `HYDRA_BACKUP_INTERVAL_SECONDS` (default 86400, minimum 60).
It has only `data-internal` access, publishes no port, and exits nonzero when
the helper or explicitly configured retention helper fails. Retention is
preview-only unless `HYDRA_BACKUP_RETENTION_APPLY=1` and
`HYDRA_BACKUP_RETENTION_CONFIRM=prune` are both supplied; it only considers
matching files in `/backups`.

The separate `vault-backup` profile reuses the built `/app/hydra-vault`
binary, mounts the active encrypted vault read-only, writes validated copies
to `hydra-vault-backups`, and uses `network_mode: none`. It requires the
operator-provided `HYDRA_VAULT_KEY`, refuses collisions, captures helper
output, and never prints values or keys. Neither profile copies artifacts
off-host or snapshots JetStream; capacity policy, key custody, off-box
replication, restore drills, and recovery ownership remain required before
production use.

## Image

`docker/Dockerfile` builds `hydra/kernel:<version>` from Rust 1.96.1, installs the pinned `wasm32-wasip2` target and `wasm-tools` 1.240.0, compiles the Kernel and memcrm adapter, and runs as non-root on Debian Bookworm slim. Its Debian build/runtime packages are pinned to exact versions. `docker/egress-proxy.Dockerfile` builds the release-matched Tinyproxy image at build time with an exact package pin; it is not installed from the package index during container startup. Migrations, WIT, and the built adapter are included. Static shell content is compiled into the binary.

The tag-triggered release workflow now requires explicit BuildKit `provenance: mode=max` and `sbom: true` settings, captures the pushed image digest, and invokes `actions/attest@v4` with the image subject and digest. Its image job requests only package, OIDC, attestation, and read-only contents permissions; organization-only storage-record creation is explicitly disabled. `bash scripts/check-release-policy.sh` validates this repository contract locally. A real signed attestation still requires an authorized GitHub tag run and is not evidenced by local validation; if hosted private-repository attestation support or permissions are unavailable, the release fails closed rather than publishing an unsigned release.

## Validation

Run the two commands recorded in `COMMANDS.md`:

```bash
docker build -f docker/Dockerfile -t hydra/kernel:local .
docker build -f docker/egress-proxy.Dockerfile -t hydra/egress-proxy:local .
docker compose --env-file docker/nexus.env.example -f docker/compose.yaml config
docker compose --profile observability --env-file docker/nexus.env.example -f docker/compose.yaml config
bash scripts/check-observability.sh
docker compose --profile backup --env-file docker/nexus.env.example -f docker/compose.yaml config
docker compose --profile vault-backup --env-file docker/nexus.env.example -f docker/compose.yaml config
```

The normalized configuration must show host `ports` only on Caddy, separate internal data/events/proxy/ingress networks, and external networks only on Caddy and the egress proxy. `bash scripts/check-container-image-policy.sh` additionally requires immutable digests for external Compose/CI images and both Dockerfile base images, plus exact package pins for the Dockerfile-installed Debian and Tinyproxy packages; only Hydra's own release-tagged images remain tag-selected before their deployment digest is verified.

## Migration And Start

1. Pin `HYDRA_TAG` to an immutable tested version and populate owner-controlled secrets.
2. Run `bash scripts/db-backup.sh`; require `backup: ok` before an upgrade.
3. Run the one-shot additive migration: `docker compose --profile ops run --rm migrate`.
4. Start or replace services with the same Compose project.
5. Require `/healthz` and `/readyz` success through Caddy, then run `bash scripts/smoke-test.sh` with `HYDRA_SMOKE_URL` set to the public URL. The external smoke requests use a five-second connect timeout and 30-second total timeout, so a stalled ingress fails the gate rather than hanging it. Do not set its internal metrics URL to the public URL; supply `HYDRA_SMOKE_INTERNAL_METRICS_URL` only when an approved internal scrape route is separately reachable. The Kernel handles Docker/orchestrator SIGTERM on Unix, drains the HTTP server, and bounds relay/executor shutdown with `HYDRA_SHUTDOWN_TIMEOUT_SECONDS`; dependency startup/readiness calls are bounded by `HYDRA_DEPENDENCY_TIMEOUT_SECONDS`.

Migrations are forward-only and run before the new kernel serves traffic. Destructive schema or data operations are STOP conditions. The migration service joins only `data-internal` and cannot reach NATS or external networks.

Migration `0016_auth_seed_hardening.sql` is additive: it preserves the historical seed row for auditability, marks it `development_seed`, and disables its form-login/session path in every environment. It does not create an owner credential or bootstrap endpoint; an operator-owned provisioning procedure remains a separate deployment prerequisite.

For a non-production archive verification, set `HYDRA_RESTORE_CONFIRM=ephemeral` and use `HYDRA_ENV=dev|staging` with `scripts/db-restore.sh`. The helper generates and cleans its own `hydra_restore_check_*` database and refuses production environment markers. This is a tool safety check, not evidence of a completed staging restore drill.

## Release And Rollback

The tag-triggered GitHub release workflow verifies, builds, and pushes only the
explicit versioned image tag, creates signed provenance for the pushed digest,
and may invoke the self-hosted staging deployment job for the repository
owner. It does not publish a mutable Hydra `latest` release alias and contains
no production deployment job. Agents must not create a tag, publish an image,
or invoke deployment without explicit authorization.

Rollback uses the prior immutable image while retaining additive schema. Follow `ROLLBACK.md`; validate the exact old-image/new-schema combination in staging first. A database restore is a separate owner-controlled recovery operation, never an implicit application rollback.

## Release Helper Safety

Actual staging execution requires the Buildx digest from the release job in
`STAGING_DIGEST` and an owner-provided `STAGING_SSH_KNOWN_HOSTS` file or
content, plus `STAGING_SMOKE_URL` naming the public HTTPS staging base URL.
`scripts/deploy-staging.sh` refuses missing or malformed digests, requires a
safe HTTPS smoke URL, uses `StrictHostKeyChecking=yes`, pulls the exact digest
before Compose, and validates `/healthz` and `/readyz` through the public Caddy
ingress. It does not probe the host-unpublished Kernel port. The tag remains
the Compose image name only; it is not the artifact proof.

Production promotion is separately authorized with `PROMOTE=yes` and an
interactive confirmation. `scripts/promote-prod.sh` requires `curl`, staging
health and readiness over credential-free HTTPS URLs, Docker, and the same
digest before tagging and pushing only the immutable `${TAG}-prod` image. It
does not create a `latest-prod` alias. Without `PROMOTE=yes` it performs a
safe dry-run and prints
`promote-gate: dry-run ok`; that marker is not an executed promotion.

`bash scripts/test-deployment-safety.sh` and
`bash scripts/check-release-policy.sh` validate these contracts locally. They
do not contact a registry, SSH host, staging URL, or production service.

## Persistence And Recovery

Authoritative CRM, audit, outbox, bindings, idempotency, and approval state live in `hydra-postgres-data`. JetStream state lives in `hydra-nats-data` and supports replay but is not the source of truth. Caddy state uses `caddy-data` and `caddy-config`; `/app/data` is reserved application-local state, including the read-only-at-runtime age vault at `/app/data/vault.age`, and must not be treated as the CRM database.

The current repository has an optional scheduled Postgres backup profile but no proven off-box replication or JetStream snapshot/restore drill. The vault is loadable, rotatable, and locally recoverable through validated `hydra-vault backup` and confirmation-gated `hydra-vault restore` commands. Owner-key custody, off-box artifact protection, and staged restore evidence remain EP-010 production-readiness gaps. See `NEXUS_PACKAGE_CONTRACT.md` for the future installer inputs and `PRODUCTION_READINESS.md` for the evidence boundary.

After a JetStream loss or stream rebuild, an owner may re-emit a bounded range
from the authoritative Postgres outbox with
`DATABASE_URL="$DATABASE_URL" NATS_URL="$NATS_URL" HYDRA_EVENT_REPLAY_CONFIRM=I_UNDERSTAND HYDRA_EVENT_REPLAY_AFTER_ID=0 HYDRA_EVENT_REPLAY_LIMIT=100 cargo run -p hydra-kernel -- --replay-events`.
The command requires a live configured stream, uses stable event IDs for
JetStream deduplication, waits for publish acknowledgement, and leaves outbox
publication state unchanged. It is not a JetStream snapshot/restore
implementation, and local replay tests do not satisfy the EP-010 staged
broker-recovery gate.

Container stop behavior is locally code-tested, but no staging termination-drain, crash-loop, or orchestrator recovery drill has been performed. Those remain EP-010 operational evidence.

## Vault provisioning

The image includes `/app/hydra-vault`. Provision the named-secret file before a staging or production kernel starts, using an operator-controlled key and stdin for each value. For the reference Compose volume, run `docker compose run --rm --entrypoint /app/hydra-vault kernel set <name>` with `HYDRA_VAULT_KEY` in the environment. Use `get-names` for inventory and set `HYDRA_VAULT_NEXT_KEY` only for an explicit rotation. For an encrypted artifact backup, run `backup <destination>` with the same key; for an explicit restore, set `HYDRA_VAULT_RESTORE_CONFIRM=restore` and run `restore <source>`. Never place secret values in Compose arguments, image layers, Git, or logs.
