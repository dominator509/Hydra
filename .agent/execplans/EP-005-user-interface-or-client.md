# EP-005 Shell (UI)

## 1. Purpose / Big Picture
Ship SPEC-004: the server-rendered shell — pipelines board, records with agent buttons, approvals queue, autonomy matrix editor, bridges tabs, agents console — progressive-enhanced htmx over plain forms.

## 2. Scope
crates/shell (Askama templates, handlers mounted by kernel), static/htmx.min.js vendored, e2e test suite real, scripts/test-e2e.sh real.

## 3. Non-goals
No JS build step; no websockets (htmx polling 5s for approvals badge); no theming; no passthrough-iframe proxy yet (EP-007 adds behind flag); no i18n.

## 4. Context and Orientation
Shell consumes fabric service traits only (layer law). Templates in crates/shell/templates; each view = handler + template + e2e path. Auth is dev-stub session until EP-006 (login form sets dev session when HYDRA_ENV=dev).

## 5. Files to Read First
SPEC-004, crates/fabric/src/services.rs, SPEC-006 (flash codes), ARCHITECTURE L4 rules.

## 6. Files to Change
crates/shell/src/{lib.rs,routes/*.rs,flash.rs,csrf.rs}, crates/shell/templates/**.html, crates/shell/static/htmx.min.js, crates/kernel/src/main.rs (mount), crates/shell/tests/e2e_*.rs, scripts/test-e2e.sh.

## 7. Interfaces and Contracts
Routes: GET /login POST /login GET / (workspace) GET/POST pipeline board+move GET entity view POST agent-button(action) GET/POST approvals(+batch) GET/POST autonomy GET bridges(+tab/:adapter) GET agents. Every POST: CSRF hidden field + htmx header variant; every list: loading/empty/error/success states per SPEC-004.

## 8. Milestones
M1 Layout+auth-stub+workspace. Validation: `cargo test -p shell --test e2e_login` → `m1: ok`. Recovery: Askama compile errors point at template line.
M2 Pipelines board + record view + agent buttons (envelope create via service; state chip renders decision). Validation: `cargo test -p shell --test e2e_pipeline` (create deal, L2 move lands in queue) → `m2: ok`. Recovery: drag needs button-alternative first — implement select-move before drag sugar.
M3 Approvals queue + batch + autonomy editor (cells post; L3 batch_max respected). Validation: `cargo test -p shell --test e2e_approvals` → `m3: ok`.
M4 Bridges tabs (origin-filtered views w/ wiring labels; memcrm fixture registered in test) + agents console (ledger sparkline via /v1/tk/ledger). Validation: `cargo test -p shell --test e2e_bridge_tab` → `m4: ok`.
M5 Degradation + a11y pass. Edits: JS-off e2e (no htmx header path), landmarks/labels assertions. Validation: `bash scripts/test-e2e.sh` → `e2e tests: ok` (script now runs all e2e_ tests, removes EP-001 temporary allowance — delete that Decision Log exemption). Recovery: JS-off failures usually = handler branching on HX-Request header; ensure full-page render branch exists.

## 9. Concrete Steps
Milestone order; screenshot notes optional in Outcomes.

## 10. Validation and Acceptance
e2e ok; verify.sh ok; TTFB spot check `curl -w '%{time_starttransfer}'` < 0.15 locally recorded in Outcomes; diff ⊆ §6.

## 11. Idempotence and Recovery
Templates hot-recompilable; e2e tests seed+teardown their tenant schema.

## 12. Progress
- [ ] M1 - [ ] M2 - [ ] M3 - [ ] M4 - [ ] M5

## 13. Surprises & Discoveries
## 14. Decision Log
## 15. Outcomes & Retrospective
