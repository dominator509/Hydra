# SPEC-007 Observability
Status: Accepted | Owner: djw | Phase: 7 | ExecPlans: EP-008
Normative details live in OBSERVABILITY.md; this spec binds acceptance.

## Goal
Operators can answer within 2 minutes: is it up, what's failing, what are agents doing, what's the token economy doing.

## Required behavior
Structured JSON logs w/ required fields + redaction allowlist; metrics set exactly as OBSERVABILITY.md incl. tk_cache_hit_ratio, tk_nuke_aborts_total; /healthz `/readyz` semantics; alert rules committed as code (docker/alerts.yaml); two dashboards JSON committed; trace spans across fabric→governor→store and agent→tk→router with prefix_sha attribute.

## Error states
Metrics endpoint failure = readyz component false; alert on scrape absence 5m.

## Required tests
smoke asserts /metrics contains 6 named series; unit test for redaction layer (secret-shaped field dropped); integration: ledger write per fake-LLM call; alert-rule yaml schema lint in security-check.

## Acceptance
`bash scripts/smoke-test.sh` ok with metric assertions; dashboards render (manual check recorded in EP-008 Outcomes).
