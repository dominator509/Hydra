# SPEC-031 - Governed Bridge Activation Conformance Gate

**Status:** Accepted for EP-051

## Purpose

Prebuilt Wasmtime bridge activation MUST prove the adapter's read-side WIT
contract before the tenant-scoped registry enters `active`. This is an
activation safety gate, not a second bridge abstraction and not a substitute
for a future canary or promotion workflow.

## Normative Requirements

1. Every new `deploy_adapter` execution that moves a registry row from
   `activating` to `active` MUST call the existing `BridgeLifecycle::conformance`
   boundary after artifact digest validation and probe, and before the active
   transition.
2. Conformance MUST use the tenant, adapter identity, component reference,
   digest, grant, and configuration already resolved by Hydra. Caller input
   MUST NOT supply a Hydra tenant, component path, digest, grant, or secret
   value.
3. The conformance call MUST remain bounded. EP-051 uses the existing host
   limit of 25 records and the adapter's first declared CRM kind when no kind
   is supplied by the activation envelope.
4. Conformance MUST exercise only the existing read-side Wasmtime/WIT
   operations. It MUST NOT call adapter mutation exports, write CRM state,
   write adapter KV state, or create an ActionEnvelope.
5. If conformance fails, Hydra MUST transition the row from `activating` to
   `failed`, preserve a sanitized bounded error, and refuse the active
   transition. The executor MUST return a failed envelope/receipt path rather
   than reporting activation success.
6. The failure transition MUST be tenant-scoped and revision-checked. A
   concurrent or stale activation MUST fail closed without overwriting a newer
   adapter state.
7. Re-deploying an already active adapter with the same digest and grant MAY
   remain idempotent. It MUST NOT change the adapter revision or bypass a
   different-component or different-grant identity conflict.
8. EP-051 MUST NOT claim generated adapter code, autonomous canarying,
   promotion, provider availability, staging readiness, or production
   readiness.

## Acceptance

- A valid fixture adapter passes conformance before becoming active.
- A fixture configuration that passes probe but fails conformance is persisted
  as `failed` and never becomes active.
- Conformance failure is tenant-scoped, redacted, and revision-audited.
- Existing idempotent redeploy and pause/resume behavior remains compatible.
- `bash scripts/preflight.sh`, focused bridge tests, `bash scripts/verify.sh`,
  and `bash scripts/check-execplan-state.sh` pass with their documented
  success markers.
