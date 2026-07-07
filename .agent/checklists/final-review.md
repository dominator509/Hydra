# Checklist: Final Review (per ExecPlan)
- [ ] Every acceptance criterion re-executed; actual output recorded beside expected.
- [ ] `bash scripts/verify.sh` → verify: ok (fresh run, not memory).
- [ ] `git diff --name-only` compared to Files to Change; extras justified in Decision Log or review FAILS.
- [ ] Docs-updated rule honored (COMMANDS/ENVIRONMENT/ARCHITECTURE/spec touched where behavior changed).
- [ ] No secrets in diff (`bash scripts/security-check.sh` includes scan).
- [ ] No production-data-touching commands were run.
- [ ] Progress all ticked; Surprises & Decision Log genuinely filled; Outcomes & Retrospective written.
- [ ] Remaining risks listed with owner/follow-up.
- [ ] Final response includes AGENTS §15 items, verbatim command results.
