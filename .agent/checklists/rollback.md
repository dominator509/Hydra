# Checklist: Rollback
- [ ] Trigger confirmed against ROLLBACK.md list (Sev1 / smoke fail / error>2% / ratio<0.90-by-release / integrity).
- [ ] Owner: operator on call decides; agents never roll back prod (STOP).
- [ ] Method chosen: image | config | autonomy-cell freeze | (rare) forward-fix migration.
- [ ] Image: set HYDRA_TAG=<prev> in docker/.env → `docker compose up -d kernel`.
- [ ] Database: NO down-migrations; forward-fix only; data repair = reviewed transactional script.
- [ ] Verify: `bash scripts/smoke-test.sh` ok; 15-min dashboard watch clean.
- [ ] Communicate: shell banner + status note with timeline.
- [ ] Postmortem scheduled ≤48h; notes filed in DECISIONS.md.
