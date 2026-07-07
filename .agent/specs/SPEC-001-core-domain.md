# SPEC-001 Core Domain (CDM + Governor)
Status: Accepted | Owner: djw | Phase: 1 | ExecPlans: EP-002

## User-visible goal
Consistent entity behavior everywhere; every agent action visibly gated by an autonomy decision users can predict from their matrix.

## Non-goals
Persistence (EP-003); HTTP; LLM calls; UI.

## Terms
Kind, Entity{id,kind,tenant,body,origin,origin_ref,version}; Edge{src,rel,dst}; Level L0..L5; Envelope states Proposed→(PendingApproval|)→Approved→Executing→Executed|Failed|RolledBack|Rejected; BlastRadius{entities,external_sends,money_cents,pii_egress}; Reversal{Compensating|Snapshot|Irreversible}.

## Required behavior
B1 Kind registry validates entity bodies against per-kind JSON Schema; unknown kind ⇒ DomainError::UnknownKind.
B2 Governor.evaluate is pure & deterministic: constitution check → cell resolve (most-specific match: exact action+kind > action > domain default) → irreversible demotion (L5→L4→L3→L2; L0/L1 fixed) → blast ceiling clamp to ≤L3 → map level to Decision {Block,SuggestOnly,Queue,Execute}.
B3 Envelope state machine rejects illegal transitions (e.g., Executing→Proposed) with DomainError::IllegalTransition; every transition records actor + rfc3339 time (injected Clock).
B4 Identity resolution: deterministic keys (email lower, phone E.164, org domain) produce MergeProposal(confidence=1.0); fuzzy candidates emit MergeProposal(confidence<1) — merges themselves are envelopes (`data.merge_parties`).
B5 Constitution checks (pure): monthly spend cap ledger input, pii_egress flag vs allowlist, hard_delete always false.

## Inputs / Outputs
In: config structs (PolicyMatrix, Constitution), Envelope drafts, entity bodies. Out: Decision, validated entities, MergeProposals, typed DomainError.

## Error states
UnknownKind, SchemaViolation{path,msg}, IllegalTransition{from,to}, PolicyResolutionAmbiguous (two equally-specific cells ⇒ config invalid, surfaced at load).

## Data rules
version increments on every mutation; body is the only free-form area and is schema-bound.

## Security rules
No IO in this layer (enforced by deps); Governor cannot be bypassed: executor in kernel requires a `Decision::Execute` token type constructed only by governor (sealed constructor).

## Required tests
proptest: demotion monotonicity (evaluate(level) never yields more permissive Decision than evaluate(level+irreversible)); matrix-resolution specificity; state-machine transition table exhaustive; schema validation fixtures per kind; constitution cap boundary (cap-1, cap, cap+1).

## Acceptance criteria
`bash scripts/test-unit.sh` → `unit tests: ok`; `cargo test -p governor --release` includes bench-ish assert p99<5ms on 10k evaluates.

## Reference implementation
Copy-adapt `reference/governor.rs` (state machine + evaluate) — normative behavior is THIS spec; the file is the fast path.
