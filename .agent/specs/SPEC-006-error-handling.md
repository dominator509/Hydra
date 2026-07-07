# SPEC-006 Error Taxonomy & Handling
Status: Accepted | Owner: djw | Phase: 3 | ExecPlans: EP-004 (+all)

## Goal
One taxonomy, machine-readable codes, predictable retries, no secret leakage.

## Taxonomy (code → HTTP → retry → user copy)
- validation_failed → 422 → no → "Check highlighted fields."
- not_found / tenant_mismatch → 404 → no
- version_conflict → 409 → client-refetch → "Record changed since you loaded it."
- authz_denied / four_eyes_violation → 403/409 → no
- rate_limited → 429 (+Retry-After) → backoff
- upstream_bridge{variant} → 502 → kernel policy: auth-expired⇒pause+alert; rate-limited⇒sleep retry-after; conflict⇒review-queue; upstream⇒exp backoff 3× then park
- llm_provider_error → 502 → router fallback chain then error
- tk_output_nuked → 502 → ONE repair retry with contract-reminder tail, then fail envelope
- tk_pii_route_blocked → 400 → no (configuration error, alert)
- constitution_blocked / cell_manual_only → 403 → no (shown with cell reference)
- internal → 500 → no, page on rate spike

## Behavior rules
fabric maps errors to problem+json {type,code,title,detail?,instance}; detail NEVER includes prompts, secrets, SQL. Shell shows flash with code. Envelope failures store the code in doc + transition row. Retries live ONLY in kernel policies above — services never sleep-loop internally.

## Logging
ERROR level for 5xx & parked bridges; WARN for 4xx spikes, nukes, ratio dips; error events carry code field for metrics `errors_total{code}`.

## Required tests
table-test taxonomy mapping; bridge variant→policy integration test (fake adapter emitting each variant); nuked-repair-once test.

## Acceptance
integration suite green; `rg 'unwrap\(\)' crates --type rust -g '!tests'` returns nothing.
