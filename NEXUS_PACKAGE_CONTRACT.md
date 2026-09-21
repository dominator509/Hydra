# Nexus Package Contract

## Purpose And Status

This is the input contract for a future Nexus installer. It packages Hydra as an independently deployable CRM/revenue subsystem; it does not put Nexus in this repository and does not authorize deployment. Local image, Compose, and fake-Nexus validation are necessary but do not satisfy EP-010 production readiness.

## Artifact

- Images: `hydra/kernel:<immutable-version>` from `docker/Dockerfile` and `hydra/egress-proxy:<immutable-version>` from `docker/egress-proxy.Dockerfile`; the tag-triggered release publishes and attests both explicit versions, and the Kernel image also includes `/app/hydra-vault` for owner-controlled provisioning.
- Entrypoint: `/app/hydra-kernel` as non-root UID 1000.
- Included contracts: additive SQL migrations, `wit/hydra-bridge.wit`, and the built memcrm WASI adapter.
- Required companion images in the reference topology: digest-pinned Postgres 16 Alpine, NATS 2.10 Alpine with JetStream, Caddy 2.8 Alpine, and the release-attested Hydra egress proxy image.
- Release policy: `.github/workflows/release.yml` requires explicit BuildKit provenance/SBOM output and a signed `actions/attest@v4` provenance attestation bound to the pushed image digest. Registry attestation is requested without the organization-only storage-record path. `bash scripts/check-release-policy.sh` validates the static contract. A tag-run attestation and registry verification remain unexecuted local evidence and must fail closed when the hosted private-repository attestation capability is unavailable.

## Modes

| Mode | Required setting | Identity boundary | Event boundary |
|---|---|---|---|
| Standalone | `NEXUS_INTEGRATION_ENABLED=false` | Hydra local auth only | Postgres remains authoritative; bundled JetStream supports Hydra runtime |
| Nexus-connected | `NEXUS_INTEGRATION_ENABLED=true` | Nexus asymmetric JWT plus active Hydra binding | `HYDRA_CRM_EVENTS_V1` and a durable Nexus consumer |

The installer starts from `docker/nexus.env.example` for Nexus-connected mode and replaces every `REPLACE_*` value. `ENVIRONMENT.md` is the exhaustive application variable contract. Required base values are `DATABASE_URL`, `NATS_URL`, `HYDRA_VAULT_KEY`, `HYDRA_BASE_URL`, and `HYDRA_ENV`; `HYDRA_VAULT_PATH` defaults to `/app/data/vault.age` in Compose. `HYDRA_ADAPTERS_PATH` is optional and enables only the prebuilt governed bridge lifecycle when the component root and vault are valid. Standalone Compose constructs its bundled database and plain development NATS URL. Nexus-connected mode must instead provide `NATS_URL` through an operator Compose override or private authenticated broker wiring; the base bundled NATS service is not a secure connected-broker substitute.

Nexus-connected mode additionally requires issuer, audience, exactly one of JWKS URL or pinned public-key file, at least one allowed MCP Origin, and `NEXUS_INTEGRATION_ENABLED=true`. Private keys, bearer tokens, bridge credentials, prompts, and customer data are never installer variables or image layers.

The optional model gateway requires all three `NEXUS_MODEL_GATEWAY_URL`, `NEXUS_MODEL_GATEWAY_MODEL`, and `NEXUS_MODEL_GATEWAY_TOKEN_SECRET`; the token is provisioned into the encrypted named-secret vault. Optional signed skill discovery requires both `HYDRA_SKILLS_PATH` and `HYDRA_SKILLS_TRUST_FILE`. The installer must mount the skill directory and trust file read-only, keep the trust file owner-controlled, and treat an invalid or partial pair as a startup failure. Skills are declarative metadata and do not receive credentials or executable tool authority.

For an offline public key, the installer mounts the public key read-only and sets `NEXUS_OIDC_PUBLIC_KEY_FILE` to its container path through an operator Compose override. It must clear `NEXUS_OIDC_JWKS_URL`. For online JWKS, Hydra caches verified keys for `NEXUS_JWKS_CACHE_SECONDS`; no per-request Nexus callback is required.

## Network Contract

| Network | Members | Internet-facing | Authority |
|---|---|---|---|
| `ingress-internal` | Caddy, kernel | no | reverse-proxy traffic only |
| `data-internal` | kernel, Postgres, migration job | no | Hydra persistence only; Nexus forbidden |
| `hydra-events` | kernel, NATS, optional trusted Nexus consumer | no | canonical event delivery only |
| `proxy-internal` | kernel, egress proxy | no | application HTTP egress hop |
| `ingress-external` | Caddy | yes | ports 80/443 only |
| `egress-external` | egress proxy | outbound | proxy-originated egress only |

The base topology publishes no kernel, Postgres, NATS client, or NATS monitoring port. A remote Nexus installation must use private networking or an authenticated private shared NATS service; public port publication is not an installation mode. Nexus never joins `data-internal`.

## Persistent Volumes

- `hydra-postgres-data`: authoritative CDM, binding, policy, audit, envelope, approval, idempotency, event, and outbox state.
- `hydra-nats-data`: bounded JetStream replay state; never the system of record.
- `caddy-data` and `caddy-config`: Caddy certificate/config state.
- `hydra-kernel-data`: reserved local application state; not the CRM database.
- `hydra-skills`: optional read-only signed `SKILL.md` packages.
- `hydra-skill-trust`: optional read-only owner-controlled Ed25519 trust policy; back it up with the owner custody procedure, never in the image.

## Migration, Health, And Readiness

Run the included image as the one-shot `migrate` service before replacing the kernel: `docker compose --profile ops run --rm migrate`. It receives only `DATABASE_URL` and joins only `data-internal`. Migrations are additive and idempotent through SQLx's migration ledger.

`GET /healthz` proves process liveness. `GET /readyz` proves required dependencies and, in Nexus-connected mode, canonical stream plus relay readiness. `GET /v1/nexus/events/status` exposes the authenticated event-contract projection. Caddy's container dependency uses kernel liveness; the installer must use readiness for traffic admission. The reference Caddyfile's `tls internal` mode is accepted only for `HYDRA_ENV=dev`; staging and production fail closed until the operator supplies a real DNS/CA Caddyfile.

## Owner And Binding Bootstrap

The installer generates, but never self-approves, an owner/bootstrap handoff containing the intended Hydra owner identity and one binding tuple: provider, external tenant ID, external business ID, Hydra tenant ID, and initial status. The one-business-to-one-Hydra-tenant v1 constraint is mandatory.

Provisioning must occur through a Hydra-owned, operator-authorized setup operation and produce append-only audit evidence. The current repository does not expose a public binding-bootstrap endpoint or installer CLI, so this remains an owner-operated prerequisite. Direct Postgres writes by Nexus, startup environment bindings, caller-selected Hydra tenant IDs, or reusable bootstrap bearer tokens are forbidden substitutes. Until a binding is provisioned, authenticated Nexus calls fail closed.

## Backup, Rollback, And Upgrade

Before migration, require `bash scripts/db-backup.sh` to report `backup: ok` and retain the backup outside the Compose volume. JetStream replay is secondary to Postgres outbox/audit state; its volume still requires an operator snapshot policy. The age vault must be backed up as an encrypted artifact with its owner key managed separately; `hydra-vault backup <destination>` validates and copies that artifact without printing its contents, and `hydra-vault restore <source>` requires explicit confirmation. Current JetStream, off-box vault custody, and staged encrypted-vault restore drills are not proven and remain EP-010 gaps.

Rollback selects the prior immutable image and retains additive schema. The installer must verify old-image/new-schema compatibility in staging and follow `ROLLBACK.md`; it must never reverse a migration or restore production implicitly. Supported upgrades are sequential tested image versions whose migrations and compatibility window pass `scripts/verify.sh`, Docker build, Compose validation, staging smoke, restore, and rollback drills.

## Installer Success Boundary

An installation is connected only when TLS is externally trusted, the issuer/audience/trust anchor validate, an active binding resolves, `/readyz` is healthy, capability discovery succeeds, and a durable Nexus consumer receives and acknowledges a canonical event. This does not by itself make Hydra production-ready; accessibility, performance, security review, soak, backup/restore, rollback, and human sign-off remain separate gates.
