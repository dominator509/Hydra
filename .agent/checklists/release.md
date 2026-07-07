# Checklist: Release
- [ ] Version chosen (SemVer) + tag prepared; WIT version bump evaluated.
- [ ] CHANGELOG.md updated (Keep-a-Changelog section for this version).
- [ ] RC criteria met: verify green on tag commit; staging soak 24h clean; cache-audit ≥0.97.
- [ ] Autonomy-default changes flagged in release notes (tenant-visible!).
- [ ] Staging smoke: `bash scripts/smoke-test.sh` → ok.
- [ ] Human approval obtained; PROMOTE=yes only by human (agents: STOP).
- [ ] Production deploy per DEPLOYMENT.md steps 1–7 (backup BEFORE migrate).
- [ ] Post-deploy verification: smoke ok; 60-min watch on golden + TOKENKILLER dashboards.
- [ ] Release notes published; tag pushed.
