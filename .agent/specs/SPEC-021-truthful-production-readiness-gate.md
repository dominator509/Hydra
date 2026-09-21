# SPEC-021 - Truthful Production-Readiness Evidence Gate

## Status

Accepted for EP-041 implementation. This specification governs the local
evidence semantics of `scripts/production-readiness-check.sh`; it does not
create staging evidence or authorize a deployment.

## Purpose

Hydra must fail closed when its production-readiness ledger contains
placeholder, pending, stale, malformed, or incomplete evidence. A row merely
existing in `PRODUCTION_READINESS.md` is not evidence that a gate passed.

## Normative Requirements

1. The EP-010 drill table in `OPERATIONS.md` MUST contain one effective
   `PASS` row for each of D1, D2, D3, D4, and D5.
2. A drill `PASS` row MUST have an ISO calendar date, non-placeholder
   metric/evidence text, and a non-placeholder operator.
3. Drill evidence MUST be no older than 30 UTC days and MUST NOT be dated in
   the future.
4. The final launch table in `PRODUCTION_READINESS.md` MUST contain exactly
   one row for each required check:
   `production-readiness-check.sh`, `Restore drill (D1)`,
   `Rollback drill (D2)`, `Nuke/cache/autonomy drills (D3-D5)`,
   `24h staging soak`, `Security review`, `Performance review`,
   `Privacy/data review`, `Accessibility review`, and `Observability review`.
5. Every required launch row MUST have an explicit `PASS` result, a
   non-placeholder owner, and a dated result no older than 30 UTC days.
6. The `Sign-off` row MUST have an explicit `PASS` result, a non-placeholder
   named owner, and a dated result no older than 30 UTC days. This records
   that the human sign-off was entered; it does not replace human judgment.
7. Matching MUST use the table's exact first-column value. Free-form prose,
   examples, `PENDING`, `BLOCKED`, `TBD`, and partial text matches MUST NOT
   satisfy a row. Drill parsing MUST be limited to the first table whose
   header is exactly `Drill | Date | Status | Metric/Evidence | Operator`, so
   procedure-section example rows cannot become evidence.
8. The evidence parser MUST be deterministic, POSIX-shell compatible, and
   independent of Hydra runtime state. The full readiness script MUST still
   run its local verification, smoke, cache, and security gates first.
9. Tests MUST prove that the current pending ledger fails, a false-green
   pending launch table fails even when D1-D5 pass, a valid fixture passes,
   stale evidence fails, and malformed or duplicate rows fail closed.
10. No test or implementation may change checked-in evidence to `PASS`, run a
    staging/production drill, or weaken the EP-010 partial status.

## Compatibility

This is a gate-hardening change only. It does not change runtime APIs,
database schemas, tenant behavior, event contracts, or public product
semantics. Existing operators may continue to append real evidence rows, but
the required table values must follow the exact machine-readable contract.
