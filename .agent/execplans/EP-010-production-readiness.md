# EP-010 Production Readiness

## 1. Purpose / Big Picture
Close the loop: execute SPEC-008 drills D1–D5 on staging, run the security/performance/privacy/accessibility reviews, verify monitoring end-to-end, complete PRODUCTION_READINESS.md with evidence, and make scripts/production-readiness-check.sh a REAL gate that verifies drill evidence + runs verify + smoke.

## 2. Scope
Drill execution + evidence logging (OPERATIONS.md table), scripts/production-readiness-check.sh real implementation, review checklists executed (results into PRODUCTION_READINESS.md), 24h staging soak kickoff+readback, launch-gate table completion EXCEPT final human sign-off row.

## 3. Non-goals
No production deploy (human-only, later); no new features or refactors — any defect found here becomes a follow-up ExecPlan unless it is a ≤5-line fix with its own regress test (Decision Log each).

## 4. Context and Orientation
SPEC-008 defines drill behavior; PRODUCTION_READINESS.md is the ledger. Staging must be deployed at a vN tag (EP-009). Live DeepSeek probe in D4 requires DEEPSEEK_API_KEY: if absent, run fake-only and record `live-sample: deferred (STOP: missing DEEPSEEK_API_KEY)` — this is a partial STOP affecting only that sub-item; continue the rest.

## 5. Files to Read First
SPEC-008, PRODUCTION_READINESS.md, OPERATIONS.md (drill table), ROLLBACK.md, scripts/production-readiness-check.sh, scripts/cache-hit-audit.sh.

## 6. Files to Change
scripts/production-readiness-check.sh (real), OPERATIONS.md (drill evidence rows D1–D5), PRODUCTION_READINESS.md (checkbox evidence + gate table rows except sign-off), .agent/state/soak-24h.md (new, soak readback), DECISIONS.md (any accepted risks).

## 7. Interfaces and Contracts
production-readiness-check.sh contract: (1) run verify.sh, (2) run smoke-test.sh, (3) run cache-hit-audit.sh, (4) grep OPERATIONS.md drill table for D1..D5 rows with PASS + ISO date <30d, (5) grep PRODUCTION_READINESS launch-gate rows non-empty except Sign-off; all pass → `production readiness: ok`, else name the first failing gate on stderr and exit 1.

## 8. Milestones
M1 Readiness script real. Validation: `bash scripts/production-readiness-check.sh; test $? -ne 0 && echo gate-fails-before-drills: ok` → expected `gate-fails-before-drills: ok` (drills not yet logged — proves the gate actually gates). Recovery: if it passed vacuously, the grep patterns are wrong — tighten to `| D<n> | .* | PASS | 20` row shape.
M2 D1 restore + D2 rollback on staging. Steps: scripts/db-backup.sh on staging → fresh volume restore via db-restore.sh → smoke; deploy vN+1 → promote → roll back to vN per ROLLBACK.md, timing both. Validation: OPERATIONS rows D1,D2 = PASS with RTO/duration; `grep -c '| D1 .* PASS' OPERATIONS.md` = 1 and same for D2 → `m2: ok`. Recovery: restore fail = launch-blocking defect → STOP-adjacent: file remediation ExecPlan, do not hand-wave.
M3 D3 nuke drill + D5 autonomy freeze. Steps: staging route pointed at dump-fake provider (compose profile drill-fakes); trigger envelope; assert tk_output_nuked after exactly one repair retry + alert fired (check alerts endpoint/log); autonomy cell L4→L1 via CLI, verify in-flight completes + new queues. Validation: OPERATIONS rows D3,D5 PASS → `m3: ok`. Recovery: two retries observed = SPEC-009 TK5 violation → regress test + fix (allowed ≤5-line rule or follow-up plan).
M4 D4 cache drill + 24h soak kickoff. Steps: cache-hit-audit against staging fake (and live 20-call sample if key present); bump one S1 segment version, verify visible ratio dip attributed via prefix_sha forensics, then revert bump; start 24h soak (agents on synthetic tenant), record start in .agent/state/soak-24h.md. Validation: `bash scripts/cache-hit-audit.sh` → `cache-hit audit: ok (ratio=0.9XX)` AND OPERATIONS row D4 PASS → `m4: ok`. Recovery: ratio <0.97 on staging but ok in CI ⇒ diff staging tk_segment_version vs repo; config drift is the usual culprit.
M5 Reviews + soak readback + gate assembly. Steps: execute security review (SECURITY checklist over last 5 PRs + grants review), performance (hey 500-req p95, governor bench, 10k import timing), privacy (export+purge demo), accessibility (keyboard + JS-off pass); after 24h, read soak metrics into soak-24h.md; fill PRODUCTION_READINESS rows; run the gate. Validation: `bash scripts/production-readiness-check.sh` → `production readiness: ok`. Expected: exactly that line. Recovery: first failing gate named on stderr → smallest remediation or documented accepted-risk ADR (only for non-critical items, never for D1/D2/security).

## 9. Concrete Steps
Milestone order; every drill leaves an OPERATIONS.md row: | Dn | <ISO date> | PASS/FAIL | <metric> | <operator> |.

## 10. Validation and Acceptance
production-readiness-check.sh ok; all PRODUCTION_READINESS sections green with evidence; launch-gate table complete except human Sign-off; final report per AGENTS §15 states explicitly: "production-readiness criteria PASSED; awaiting human sign-off + PROMOTE=yes".

## 11. Idempotence and Recovery
Drills re-runnable (each targets scratch DBs/synthetic tenants); evidence rows append-dated; resume = check which D-rows exist.

## 12. Progress
- [ ] M1 - [ ] M2 - [ ] M3 - [ ] M4 - [ ] M5

## 13. Surprises & Discoveries
## 14. Decision Log
## 15. Outcomes & Retrospective
