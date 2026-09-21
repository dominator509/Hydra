# EP-053 Shell Accessibility and Degradation
Plan status: COMPLETE

## 1. Purpose / Big Picture
Close the code-owned portion of EP-005 M5 and SPEC-033. Replace the two
critical JavaScript-only disclosure controls with native semantic disclosures,
add a skip link and explicit landmarks, and enforce a deterministic source
contract for no-JavaScript mutation fallbacks.

## 2. Scope
Only the existing server-rendered Shell templates, its local contract test and
gate, and the documentation/status surfaces needed to make the evidence
truthful. Existing routes, action URLs, CSRF behavior, htmx enhancement, and
the no-Node architecture remain unchanged.

## 3. Non-goals
No browser automation framework, screenshot review, screen-reader claim,
contrast certification, staging deployment, redesign, new JavaScript library,
new dependency, or change to CRM/Governor behavior. Do not mark EP-010
production-ready from local template evidence.

## 4. Context and Orientation
SPEC-004 requires semantic landmarks, label-bound forms, focus visibility, a
select alternative for movement, and native form behavior when JavaScript is
off. EP-005 records M5 as unexecuted. Current templates already preserve most
native form actions, but the New Deal and Kind Overrides disclosures use
`onclick` and hidden CSS, and no Shell accessibility contract test exists.

## 5. Files to Read First
`AGENTS.md`; `COMMANDS.md`; `.agent/PLANS.md`;
`.agent/EXECUTION_RULES.md`; `SPEC-004-ui-ux-behavior.md`;
`.agent/specs/SPEC-033-shell-accessibility-degradation.md`;
`.agent/execplans/EP-005-user-interface-or-client.md`;
`crates/shell/templates/layout.html`;
`crates/shell/templates/nav.html`;
`crates/shell/templates/pipeline_board.html`;
`crates/shell/templates/autonomy.html`;
`crates/shell/templates/action_button.html`;
`crates/shell/templates/approval_row.html`;
`crates/shell/templates/bridges.html`;
`crates/shell/templates/record_view.html`;
`crates/shell/src/routes/*.rs`; `scripts/preflight.sh`;
`scripts/verify.sh`; `TESTING.md`; `PRODUCTION_READINESS.md`.

## 6. Files to Change
`.agent/specs/SPEC-033-shell-accessibility-degradation.md`;
`.agent/execplans/EP-053-shell-accessibility-and-degradation.md`;
`.agent/state/execplan-index.md`; `scripts/check-execplan-state.sh`;
`crates/shell/templates/layout.html`;
`crates/shell/templates/pipeline_board.html`;
`crates/shell/templates/autonomy.html`;
`crates/shell/templates/components/flash.html`;
`crates/shell/tests/accessibility_contract.rs`;
`scripts/test-shell-accessibility.sh`; `scripts/preflight.sh`;
`scripts/verify.sh`; `COMMANDS.md`; `TESTING.md`;
`.agent/specs/SPEC-004-ui-ux-behavior.md`;
`.agent/execplans/EP-005-user-interface-or-client.md`;
`PRODUCTION_READINESS.md`; `DECISIONS.md`.

## 7. Interfaces and Contracts
The Shell keeps all existing endpoints and form names. New Deal and Kind
Overrides use `details`/`summary` and remain ordinary server-rendered forms.
The contract test is a Rust integration test with no network, database, or
browser dependency. The gate invokes that test and emits one exact success
marker: `shell accessibility: ok`.

## 8. Milestones
M1 - Add the accepted SPEC and activate this plan in the status index.
Validation: `bash scripts/check-execplan-state.sh` fails only until the new
plan is present in its required loop, then recognizes exactly one ACTIVE plan.

M2 - Implement semantic skip-link, landmark, disclosure, and flash-control
changes without changing routes. Validation: focused Shell contract test.

M3 - Add the strict Shell accessibility gate and wire it into preflight and
the full verifier; update command/testing/readiness documentation. Validation:
`sh scripts/test-shell-accessibility.sh` -> `shell accessibility: ok`.

M4 - Reconcile EP-005 and complete the plan after focused and full local gates
pass. Validation: `bash scripts/verify.sh` -> `verify: ok`, state checker ->
`execplan state: ok`, formatter and diff checks pass.

## 9. Concrete Steps
1. Add the native contract test using `include_str!` for the checked-in
   templates and assert semantic landmarks, skip target, native disclosures,
   and required action/method pairs for every enhanced Shell form.
2. Replace the New Deal JavaScript toggle with a `details` disclosure.
3. Replace the Kind Overrides overlay/toggle/close path with a native
   `details` disclosure that keeps the existing POST form and accessible
   heading.
4. Add the skip link, remove the duplicate outer navigation role, and make
   the flash dismiss button an explicit button type.
5. Add the gate to the required script inventory and `verify.sh`; document its
   local-only boundary and append current verification to EP-005 and the
   readiness ledger.

## 10. Validation and Acceptance
Run, in order:

1. `cargo test -p shell --test accessibility_contract --offline -- --nocapture`
2. `sh scripts/test-shell-accessibility.sh`
3. `bash scripts/check-execplan-state.sh`
4. `cargo fmt --all -- --check`
5. `git diff --check`
6. The resource-safe full verifier from `AGENTS.md`, which must exit zero and
   print `verify: ok`.

Expected focused output includes `shell accessibility contract: ok` and
`test result: ok`; the wrapper must print `shell accessibility: ok`.

## 11. Idempotence and Recovery
The changes are deterministic text/template contracts. Re-running the focused
test and wrapper is safe and has no database or network side effects. If
Askama rejects a template, restore the smallest semantic markup change while
keeping the native `action`/`method` fallback and rerun the focused test. Do
not add a parser dependency to bypass a template error.

## 12. Progress
- [x] M1 - SPEC, plan, and status activation
- [x] M2 - Semantic Shell changes and focused contract
- [x] M3 - Required gate and documentation wiring
- [x] M4 - Full validation and truthful reconciliation

## 13. Surprises & Discoveries
- 2026-08-12 - EP-005 M5 is still explicitly unexecuted. The current E2E gate
  proves Nexus system behavior but intentionally does not prove browser or
  Shell accessibility behavior.
- 2026-08-12 - The New Deal and Kind Overrides controls are the concrete
  JavaScript-only disclosure paths; their underlying mutation forms already
  retain native POST actions.

## 14. Decision Log
- 2026-08-12 - Use native `details`/`summary` rather than adding JavaScript or
  a browser dependency. This is the smallest standards-based correction that
  makes the controls keyboard- and no-JavaScript-capable while preserving
  existing routes and form contracts.
- 2026-08-12 - Add a static Rust contract gate as local evidence, but keep
  browser/staging accessibility review open in EP-010. A source contract
  cannot certify screen-reader output or real contrast.

## 15. Outcomes & Retrospective
EP-053 is complete. Native `details`/`summary` controls replaced the
JavaScript-only New Deal and Kind Overrides disclosures, a skip link and
explicit navigation landmark were added, and the Shell contract rejects
nested forms or missing native POST fallbacks. The autonomy disclosure was
kept outside the matrix form so the rendered HTML remains valid.

Evidence:

- `bash scripts/check-execplan-state.sh` -> `execplan state: ok` while EP-053
  was active.
- `bash scripts/test-shell-accessibility.sh` -> `shell accessibility: ok`;
  one focused contract test passed.
- `cargo fmt --all -- --check`, `git diff --check`, and `sh -n` passed.
- The resource-safe `bash scripts/verify.sh` exited 0 in 862.8 seconds on
  2026-08-12, including the required Shell gate.

This is local source/template evidence only. Browser keyboard traversal,
screen-reader output, contrast, staging no-JavaScript review, and all other
EP-010 production-readiness evidence remain open. No production deployment,
push, tag, or production database operation occurred.
