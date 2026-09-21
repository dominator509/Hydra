# SPEC-025 Event Listener Advisory Remediation

## Status

Accepted for EP-045. This specification removes the known transitive
`event-listener` unsoundness from Hydra's lockfile without changing the
application architecture or SQLx facade.

## Security requirement

Hydra MUST NOT ship `event-listener` versions affected by
`RUSTSEC-2026-0221`. The lockfile MUST resolve `event-listener` to a patched
release at or above `5.4.2`, or to an explicitly newer unaffected release
accepted by Cargo's resolver. The remediation MUST preserve the current
Postgres-only vendored SQLx dependency boundary and all existing features.

## Scope and constraints

- Use the smallest compatible Cargo lockfile update.
- Do not add a direct application dependency merely to force resolution.
- Do not replace SQLx, Tokio, Wasmtime, or the async runtime.
- Do not add an advisory ignore for `RUSTSEC-2026-0221`.
- Preserve checked SQLx metadata and all current test behavior.
- No production database, deployment, tag, push, or external provider call.

## Acceptance

- `Cargo.lock` resolves a patched `event-listener` release.
- `cargo tree -i event-listener --target all --offline` shows the expected
  transitive SQLx path and no duplicate vulnerable version.
- `cargo audit` reports no vulnerability and no `RUSTSEC-2026-0221` warning.
- `cargo deny check` passes advisories, bans, licenses, and sources.
- Existing preflight and full verifier gates pass.
- `PRODUCTION_READINESS.md` no longer lists `event-listener` as an open
  supply-chain residual, while unrelated residuals remain explicit.
