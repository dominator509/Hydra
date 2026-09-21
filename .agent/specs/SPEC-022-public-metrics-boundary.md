# SPEC-022 - Public Metrics Boundary

## Status

Accepted for EP-042 implementation. This specification hardens the reference
ingress topology; it does not claim that live dashboards, alert delivery, or
staging observability review have occurred.

## Purpose

Hydra's Kernel exposes a bounded process-local Prometheus surface for the
internal observability profile. The public Caddy listener must not forward
that diagnostic endpoint to unauthenticated internet or Nexus clients.

## Normative Requirements

1. The public Caddy listener MUST deny the `/metrics` path prefix before the
   catch-all Kernel reverse proxy. The protected prefix is `/metrics*`, which
   also covers path variants such as `/metrics/`.
2. The public denial MUST return a non-success response and MUST NOT proxy the
   request to Kernel. The reference policy uses `404` to avoid disclosing the
   existence of an operational endpoint.
3. The internal Prometheus service MUST continue to scrape `kernel:8080/metrics`
   over `ingress-internal`; this change MUST NOT remove internal observability.
4. Kernel MUST remain un-published to host ports in the reference Compose
   profile. Caddy remains the only public ingress service.
5. The Caddy configuration MUST express the boundary with an explicit named
   matcher and a dedicated `handle` block before the catch-all proxy handle.
6. A repository policy check MUST reject configurations missing the matcher,
   denial, or internal proxy structure. Fixture tests MUST prove both the
   accepted topology and representative fail-closed mutations.
7. No metrics label, response, or policy test may introduce tenant IDs,
   customer data, secrets, access tokens, or prompts.
8. Documentation MUST distinguish local configuration validation from live
   staging authentication, dashboards, alert delivery, and operator review.

## Compatibility

Internal Prometheus scraping and the direct Kernel route are unchanged. Public
clients that previously reached `/metrics` receive `404`; this is an
intentional security boundary correction. Health, readiness, MCP, REST, and
normal shell paths continue through the catch-all proxy handle.
