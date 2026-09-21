# SPEC-032 - Required Local Performance Evidence

**Status:** Accepted for EP-052

## Purpose

Hydra MUST run its existing performance-oriented checks as a required,
fail-fast local gate. A green unit suite alone is insufficient evidence for
Governor latency, large bridge imports, or TOKENKILLER cache discipline.

## Normative Requirements

1. The required performance gate MUST run the release-only Governor p99 test
   `perf_governor_eval_p99_under_5ms`.
2. The required performance gate MUST discover and pass the named 10,000-record
   bridge conformance test `c9_soak_10k` through the existing wrapper.
3. The required performance gate MUST run the existing TOKENKILLER replay
   cache-hit audit and enforce `TK_HIT_RATIO_TARGET` (default `0.97`).
4. Any missing test, nonzero command, failed assertion, or missing success
   marker MUST fail the gate. No `|| true`, ignored failure, or
   `continue-on-error` is permitted.
5. `scripts/verify.sh` MUST invoke the performance gate. The nightly workflow
   MUST invoke the same gate rather than maintaining a weaker duplicate path.
6. The gate MUST emit only bounded success markers and test summaries. It MUST
   not print prompts, secrets, customer records, or provider payloads.
7. The results establish local executable performance evidence only. They MUST
   NOT satisfy EP-010 staging latency, 24-hour soak, live provider budget, or
   human performance-review requirements.

## Acceptance

- `bash scripts/test-performance.sh` emits `performance: ok` after all three
  checks pass.
- `bash scripts/verify.sh` invokes the gate and remains fail-fast.
- Nightly policy statically proves the shared gate is used and no masked path
  exists.
- A missing or failing named test produces a nonzero exit.
