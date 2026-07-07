# Checklist: Preflight (mechanical, before first edit)
- [ ] `git status --porcelain` clean or only expected files; on repo root.
- [ ] `bash scripts/preflight.sh` → `preflight: ok`.
- [ ] Toolchain: `cargo --version` matches rust-toolchain.toml; clippy+fmt installed.
- [ ] Deps installed: `bash scripts/install.sh` run this machine (→ `install: ok`).
- [ ] Test harness alive: `cargo test -p cdm --lib -- --list` exits 0 (post-EP-002).
- [ ] Local services if plan needs them: `docker compose -f docker/compose.yaml up -d postgres nats` healthy.
- [ ] Required env vars for this plan present per ENVIRONMENT.md (DATABASE_URL, NATS_URL; provider keys only if plan says live).
- [ ] Required secrets: absent+required ⇒ STOP now, not mid-plan.
- [ ] Known blockers from prior plan's Outcomes reviewed.
