# DEPLOYMENT.md

## Environments
dev (local compose) → staging (single VPS, real TLS, fake tenants) → prod (single box v1).

## Deployment architecture
Caddy (:443 TLS) → hydra-kernel (:8080, serves shell+API) ; postgres ; nats ; optional llama-server ; egress-proxy container is the ONLY container with outbound internet besides Caddy.

## Build artifact
Multi-stage Dockerfile → image `hydra/kernel:<version>` (static-ish, distroless base); adapters/*.wasm baked read-only; SBOM emitted (`cargo auditable` metadata).

## Release flow
tag `vX.Y.Z` → CI runs scripts/verify.sh → builds+pushes image → staging auto-deploy → smoke → manual promote to prod (deliberate human step; see RELEASE.md).

## Deployment steps (staging & prod)
1. `git fetch --tags && git checkout vX.Y.Z`
2. `docker compose -f docker/compose.yaml pull`
3. Backup: `bash scripts/db-backup.sh` (created EP-008) — must print `backup: ok`.
4. `cargo sqlx migrate run` via one-shot container `docker compose run --rm migrate` — expect `Applied N migrations`.
5. `docker compose up -d`
6. `bash scripts/smoke-test.sh` → `smoke test: ok`
7. Watch `tk_cache_hit_ratio` and error rate 15 min (OBSERVABILITY dashboards).

## Migration steps
Forward-only; run BEFORE new image serves traffic (step 4 before 5). Destructive change = STOP (permission required) + rehearsed on staging with restore drill.

## Rollback
See ROLLBACK.md. Short: `docker compose down kernel && docker compose up -d kernel@previous-tag`; DB migrations are additive so old code runs on new schema.

## Post-deploy smoke
scripts/smoke-test.sh: /healthz 200, /readyz 200, login flow, create+read entity, governor L2 queue round-trip, TK ledger endpoint returns ratio.

## Approvals & STOP
Prod deploy requires explicit human `PROMOTE=yes` env on the promote script. Agents: production deployment without explicit permission is a STOP condition.
