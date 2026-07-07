# SPEC-008 Production Readiness
Status: Accepted | Owner: djw | Phase: 9 | ExecPlans: EP-010
PRODUCTION_READINESS.md is the checklist; this spec defines the drills' required behavior.

## Required drills (each scripted, each leaves evidence in OPERATIONS.md log table)
D1 Restore: fresh volume ← last nightly dump; smoke green; RTO ≤ 30min measured.
D2 Rollback: deploy vN on staging, promote vN+1, roll back to vN in ≤10min, smoke green.
D3 Nuke drill: inject 1MB-dump fake provider on a staging route; assert envelope fails with tk_output_nuked after exactly one repair retry; alert fired.
D4 Cache drill: run replay corpus (tests/fixtures/tk-corpus/) against staging deepseek fake AND (if key present) 20-call live sample; ratio ≥0.97; then bump a S1 segment version and verify ratio dip is visible + attributed via prefix_sha in cache forensics.
D5 Autonomy freeze: Auditor agent (or CLI) drops a cell L4→L1; verify in-flight L4 envelopes complete but new ones queue.

## Error states
Any drill failure = launch blocked; remediation ExecPlan required.

## Acceptance
`bash scripts/production-readiness-check.sh` → `production readiness: ok` (script verifies drill evidence rows exist and are <30 days old, plus runs verify + smoke).
