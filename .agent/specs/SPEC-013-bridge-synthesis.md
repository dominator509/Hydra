# SPEC-013: Governed Bridge-Mapping Synthesis

Status: Accepted for EP-033
Version: 1.0

## 1. Purpose

Define the narrow BridgeEngineer synthesis seam that turns bounded adapter
metadata into a reviewable mapping proposal without generating, loading, or
executing adapter code. Hydra remains the authority for the Wasmtime/WIT ABI,
Governor decisions, Store persistence, and bridge activation.

## 2. Required Call Path

The only model-backed path is:

`authenticated A2A bridge-synthesis task -> Kernel BridgeEngineer runtime -> agents::BridgeEngineer -> TOKENKILLER Session -> Hydra llm-router`

The `agents` crate may depend on TOKENKILLER's provider-neutral Session
contract, but never imports `llm-router`, credentials, Store, Wasmtime, or
vendor SDKs. The Kernel supplies the configured router and Store ledger sink.

## 3. Input Contract

The A2A task input is a bounded JSON object containing:

- `adapterId`: safe identifier, 1-128 bytes
- `descriptor`: bounded adapter descriptor metadata, at most 8 KiB
- `schema`: bounded schema/introspection metadata, at most 16 KiB

Inputs contain no access tokens, prompts, secrets, raw customer records,
email bodies, arbitrary URLs, or executable source. The task is tenant-scoped
and idempotent by the existing A2A `(tenant_id, message_id, request_hash)`
contract.

## 4. TOKENKILLER Contract

- Route: `bridge_mapping`
- Contract: `MappingYaml`
- PII: false; customer bodies are rejected before the route
- Stable S0-S2 segments are fixed and versioned; dynamic metadata is in S3
- NukeGuard, one repair retry, ledger recording, and provider provenance are
  mandatory through `tokenkiller::Session`
- Output is bounded and contains no Markdown fences

## 5. Mapping Proposal Contract

The validated mapping has exactly the top-level semantic fields `adapter`,
`entity`, and `fields`. Adapter identity must match the request. Entity and
field names are bounded safe identifiers. At least one and at most 64 fields
are allowed. URLs, secret-shaped keys, traversal markers, and executable
content fail closed.

The result is a proposal artifact containing the normalized mapping, bounded
validation report, TOKENKILLER repair flag, and redacted provider/model
provenance. It is not a Wasm artifact, activation record, ActionEnvelope, or
approval assertion.

## 6. A2A Boundary

`bridge-synthesis` is available only when the Kernel has a configured
TOKENKILLER provider chain and `bridge_mapping` route. The authenticated A2A
workflow persists a tenant-scoped task, transitions `submitted -> working ->
completed|failed`, and returns the mapping proposal as a bounded artifact.
Equivalent retries return the same task; conflicting message reuse fails.
Nexus agents may request synthesis but cannot approve, activate, or execute
the proposal.

## 7. Explicit Non-goals

- No generated Wasm or source-code execution.
- No direct Wasmtime instantiation from the agent or A2A path.
- No bridge deploy, pause, resume, synchronization, conformance, canary, or
  promotion mutation.
- No new CRM abstraction or Store table.
- No production deployment, live Nexus dependency, or production database.

## 8. Acceptance

- Missing or malformed input fails closed without a provider call.
- Provider output passes through TOKENKILLER and strict mapping validation.
- Nuke/contract failure produces no executable bridge artifact.
- A2A discovery reports availability truthfully.
- Tenant isolation, idempotency, failure state, and correlation are preserved.
- Existing standalone mode remains unchanged when no provider is configured.
