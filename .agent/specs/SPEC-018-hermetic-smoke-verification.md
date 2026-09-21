# SPEC-018 - Hermetic Smoke Verification

## Status

Accepted for EP-038. This specification defines the local smoke-test
boundary; it does not alter production deployment or database behavior.

## Purpose

The required Hydra verifier must prove Kernel health and readiness against an
isolated, migrated test schema rather than depending on a pre-existing root
database. A smoke run MUST be repeatable on a loopback PostgreSQL instance
with no prior schema state.

## Requirements

1. The smoke harness MUST require a loopback-compatible `DATABASE_URL` using
   the same safety boundary as the integration harness.
2. The harness MUST create a unique PostgreSQL schema and apply the checked-in
   Store migrations before starting the Kernel child process.
3. The child process MUST receive a connection URL whose `search_path` is
   restricted to the unique test schema plus `public`.
4. The harness MUST preserve the existing `NATS_URL` contract and MUST NOT
   create, mutate, or delete production NATS or PostgreSQL state.
5. The harness MUST shut down the child and drop the test schema on success,
   endpoint timeout, child failure, or assertion failure.
6. The child MUST continue to use the real Kernel binary and the existing
   `/healthz`, `/readyz`, and `/readyz/details` assertions.
7. A missing or unavailable Store authority MUST remain a failure; the test
   MUST NOT bypass the Store-backed rate limiter or readiness checks.
8. The schema-scoped URL helper MUST preserve connection credentials without
   logging or returning them in test output.
9. The command and testing documentation MUST state that smoke verification
   uses a disposable migrated schema.

## Non-goals

- No production database migration or deployment.
- No change to health, readiness, rate-limit, or authentication semantics.
- No shared test schema, hard-coded tenant, or destructive operation outside
  the generated disposable schema.
