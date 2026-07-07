# EP-009 Deployment & Release

## 1. Purpose / Big Picture
Turn HYDRA into a shippable artifact: multi-stage Dockerfile, full compose topology (Caddy TLS, egress-proxy, optional llama-server), release CI (tag → verify → build → push → staging deploy → smoke), migrate one-shot container, promote gate, rollback path rehearsed on staging — plus the n8n node package artifact.

## 2. Scope
docker/Dockerfile (real), docker/compose.yaml (full topology + profiles), docker/Caddyfile, docker/egress-proxy config, .github/workflows/release.yml, scripts/deploy-staging.sh + scripts/promote-prod.sh (new, PROMOTE=yes gate), migrate service, CHANGELOG.md bootstrap, n8n/ package dir (Hydra Trigger + Hydra Action node JSON+manifest built as a tarball artifact — no npm in THIS repo; nodes ship as source archive consumers install into their n8n).

## 3. Non-goals
No prod deployment execution (STOP without permission — this plan only rehearses staging); no k8s manifests beyond docs stub; no registry hosting decisions beyond a REGISTRY env var; no autoscaling.

## 4. Context and Orientation
DEPLOYMENT.md/RELEASE.md/ROLLBACK.md are normative. Staging = any Linux host with docker compose + DNS; CI deploys via ssh (DEPLOY_SSH_* secrets — absence at execution time is a STOP for the live-deploy milestone only; everything else proceeds).

## 5. Files to Read First
DEPLOYMENT.md, RELEASE.md, ROLLBACK.md, docker/compose.yaml, .github/workflows/ci.yml, ENVIRONMENT.md (env table).

## 6. Files to Change
docker/Dockerfile, docker/compose.yaml, docker/Caddyfile, docker/egress-proxy.yaml (or squid/tinyproxy conf — smallest: tinyproxy with allowlist), .github/workflows/release.yml, scripts/deploy-staging.sh, scripts/promote-prod.sh, CHANGELOG.md, n8n/{hydra-trigger.node.json,hydra-action.node.json,README.md,package-manifest.json}, docker/.env.example (HYDRA_TAG, REGISTRY), DEPLOYMENT.md (fill any discovered gaps only).

## 7. Interfaces and Contracts
Image `$REGISTRY/hydra/kernel:$TAG`; compose services: caddy, kernel, postgres, nats, egress-proxy, migrate (profile=ops, one-shot `sqlx migrate run`), cron, llama-server (profile=local-llm). Only caddy+egress-proxy have outbound network (compose networks: frontnet, backnet-internal). release.yml: on tag v* → verify.sh → docker build → push → ssh staging `deploy-staging.sh $TAG` → remote smoke. promote-prod.sh refuses unless `PROMOTE=yes` env AND tty confirmation.

## 8. Milestones
M1 Dockerfile builds. Exact edits: stage1 rust:1.79 cargo build --release + build-adapters; stage2 gcr.io/distroless/cc, copy hydra bin + adapters/ + migrations/ + static/. Validation: `docker build -f docker/Dockerfile -t hydra/kernel:dev . && echo m1: ok`. Expected: `m1: ok`. Recovery: distroless missing libs → check ldd on binary in stage1; prefer static openssl-vendored feature (Decision Log).
M2 Full compose topology + network isolation. Validation: `docker compose -f docker/compose.yaml config >/dev/null && docker compose -f docker/compose.yaml up -d && bash scripts/smoke-test.sh` → `smoke test: ok`; plus isolation probe: `docker compose exec kernel wget -qO- --timeout=3 https://example.com || echo egress-blocked: ok` → `egress-blocked: ok`. Recovery: kernel needs egress ONLY via proxy env HTTP_PROXY=http://egress-proxy:8888 — set in compose, verify router honors it.
M3 Migrate one-shot + Caddy TLS. Validation: `docker compose --profile ops run --rm migrate` → `Applied N migrations` (or 0); `curl -sk https://localhost/healthz` via caddy self-signed local → `ok`. Recovery: caddy local CA: use `tls internal` directive in dev block.
M4 Release CI + staging deploy scripts. Validation: `bash scripts/deploy-staging.sh --dry-run vTEST` → prints plan + `deploy-staging: dry-run ok`; `PROMOTE=no bash scripts/promote-prod.sh vTEST; test $? -ne 0 && echo promote-gate: ok`. Expected: both ok lines. Recovery: live ssh deploy validated only if DEPLOY_SSH_HOST set; else record deferred in Decision Log (not a STOP for dry-run milestone).
M5 n8n artifact + CHANGELOG + rollback rehearsal note. Edits: n8n nodes calling REST v1 (trigger = webhook subscribe /v1/webhooks; action = propose_envelope), packaged `tar czf dist/hydra-n8n-nodes.tgz n8n/`; CHANGELOG bootstrap; append staging rollback rehearsal steps to ROLLBACK verification section. Validation: `tar tzf dist/hydra-n8n-nodes.tgz | grep -c node.json` ≥2 → `m5: ok`. Recovery: tar path issues → run from repo root per COMMANDS rule.

## 9. Concrete Steps
Order above; commit per milestone; never run promote against a real host in this plan.

## 10. Validation and Acceptance
verify.sh green; docker build ok; compose smoke ok + egress isolation ok; migrate one-shot ok; promote gate refuses without PROMOTE=yes; n8n tarball lists 2 nodes; diff ⊆ §6. STOP reminder: actual production deploy is out of scope and forbidden without explicit permission.

## 11. Idempotence and Recovery
Builds cached; compose up idempotent; deploy script re-runnable (pull+up -d); resume = rerun milestone validation.

## 12. Progress
- [ ] M1 - [ ] M2 - [ ] M3 - [ ] M4 - [ ] M5

## 13. Surprises & Discoveries
## 14. Decision Log
## 15. Outcomes & Retrospective
