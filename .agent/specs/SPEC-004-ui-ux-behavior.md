# SPEC-004 Shell UI/UX Behavior
Status: Accepted | Owner: djw | Phase: 4 | ExecPlans: EP-005

## Goal
Server-rendered shell: fast, keyboard-friendly, degradable; agentic affordances on every record; bridge tabs indistinguishable from native ones.

## Non-goals
SPA framework; mobile apps; theming engine; Node tooling.

## Screens & flows
Login → Workspace: left nav [Pipelines, Parties, Activities, Tickets, Campaigns, Bridges(tab per adapter), Approvals, Agents, Autonomy, Audit, TK Dashboard-link].
Pipeline board: columns=stages, cards=deals, htmx drag→POST /move (envelope if cell requires). Record view: fields + activity stream + agent buttons [Draft follow-up, Score, Enrich] → creates envelope; button shows resulting state (Suggested/Queued/Executed) inline.
Approvals: queue list; row expand shows rationale, blast radius, diff preview; Approve/Reject buttons; batch approve for L3 bundles with count confirmation ("Approve 25 actions").
Autonomy: matrix editor (domain rows × action cols; kind override drawer); changing a cell posts and flash-confirms; L4/L5 cells show shield icon + constitution summary.
Bridges: list with status chips (Syncing/Parked/Canary); tab per promoted adapter renders CDM views filtered origin=bridge:<id> with legacy display labels from wiring; "Passthrough" subtab iframes proxy when configured.
Agents console: per-agent last actions, route, tk ratio sparkline.

## States (every list view)
loading (htmx indicator), empty (explicit CTA text), error (flash + retry link), success.

## Accessibility
Semantic landmarks; forms label-bound; drag-drop has button alternative ("Move to stage…" select); focus visible; contrast AA; ALL htmx posts work as normal form posts when JS off (progressive enhancement is a test, not a hope).

## Error states
Flash messages carry SPEC-006 code + human text; 403 shows role needed.

## Security
CSRF token on all mutations; no entity data in URLs beyond ids; session per SECURITY.md.

## Performance
TTFB p95 <150ms; template render <20ms typical.

## Required tests
E2E script drives: login, create deal, move stage at L2 → appears in Approvals → approve → board updates; bridge tab lists fixture records; JS-off pass for create deal.

## Acceptance
`bash scripts/test-e2e.sh` → `e2e tests: ok`.
