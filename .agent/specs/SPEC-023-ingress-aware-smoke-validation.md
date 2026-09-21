# SPEC-023 - Ingress-Aware Smoke Validation

## Status

Accepted for EP-043 implementation. This specification aligns smoke testing
with the EP-042 public metrics boundary and does not claim a staging run.

## Purpose

The public smoke branch is intended to exercise an externally reachable Caddy
URL. Once `/metrics*` is correctly private, that branch must not treat a
public `404` as a service failure or encourage operators to publish metrics.

## Normative Requirements

1. `HYDRA_SMOKE_URL` MUST remain the public/ingress URL used for `/healthz`
   and `/readyz` checks.
2. The public smoke branch MUST NOT request `/metrics` from
   `HYDRA_SMOKE_URL`.
3. An optional `HYDRA_SMOKE_INTERNAL_METRICS_URL` MAY identify a separately
   reachable internal Kernel or Prometheus metrics URL. When supplied, the
   smoke test MUST fetch and validate the required bounded series from that
   URL, not from the public URL.
4. The smoke test MUST reject an internal metrics URL identical to the public
   smoke URL, preventing accidental reintroduction of the public path.
5. The no-`HYDRA_SMOKE_URL` in-repository smoke path MUST continue to validate
   the real Kernel's direct `/metrics` endpoint because it is an internal
   process contract.
6. The smoke output MUST explicitly state when public smoke skips metrics due
   to the internal-only boundary; skipping MUST NOT be presented as a metrics
   pass.
7. Fixture tests MUST prove public health/readiness requests, absence of a
   public metrics request, optional internal metrics validation, and equal-URL
   rejection.

## Compatibility

Existing callers may continue setting `HYDRA_SMOKE_URL`; they must add
`HYDRA_SMOKE_INTERNAL_METRICS_URL` only when they have an approved internal
metrics route. Local in-repository smoke behavior is unchanged.
