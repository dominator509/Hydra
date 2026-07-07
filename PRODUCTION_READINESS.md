# PRODUCTION_READINESS.md

## Definition
HYDRA is production-ready when every section below is green, `bash scripts/production-readiness-check.sh` prints `production readiness: ok`, and the launch gate sign-off row is completed.

## Functional readiness
[ ] All SPEC-000 core outcomes demonstrable in staging  [ ] All specs' acceptance criteria pass  [ ] Non-goals still excluded (diff audit)  [ ] Known criticals resolved or accepted in DECISIONS.md

## Test readiness
[ ] verify.sh green 3× consecutive  [ ] conformance nightly (incl. 10k soak) green  [ ] regression suite covers: envelope lifecycle, bridge round-trip, PII gate, NukeGuard abort, cache replay ≥0.97

## Security readiness
[ ] security-check.sh green  [ ] authz matrix tests green  [ ] grants reviewed  [ ] vault backed up  [ ] session/CSRF verified  [ ] SECURITY checklist on last 5 PRs

## Privacy/data readiness
[ ] export + soft-delete purge demonstrated  [ ] retention jobs scheduled  [ ] backup nightly, restore drill PASSED with timestamp logged in OPERATIONS.md

## Performance readiness
[ ] shell p95 <150ms (hey loop 500 req)  [ ] governor bench <5ms p99  [ ] 10k-record import <10min on staging  [ ] tk_cache_hit_ratio ≥0.97 sustained 24h on staging agents

## Accessibility readiness
[ ] keyboard-only pass of core flows  [ ] landmarks/labels audit  [ ] no-htmx degradation pass

## Observability readiness
[ ] dashboards live  [ ] alerts firing tested (synthetic)  [ ] logs redacted (grep for key patterns = none)

## Deployment/rollback readiness
[ ] staging deploy from tag reproducible  [ ] rollback drill executed (<10min to previous tag)  [ ] migration additivity confirmed

## Documentation/support readiness
[ ] OPERATIONS runbook current  [ ] RELEASE/ROLLBACK current  [ ] incident checklist printed/pinned  [ ] escalation path named

## Final launch gate
| Check | Owner | Date | Result |
|---|---|---|---|
| production-readiness-check.sh | | | |
| Restore drill | | | |
| Rollback drill | | | |
| 24h staging soak (ratio+errors) | | | |
| Sign-off | djw | | |
