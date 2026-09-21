# SPEC-024 Bridge Egress Runtime Wiring

## Status

Accepted for EP-044. This specification closes the runtime wiring gap between
the explicit Hydra egress proxy contract and configured Wasmtime bridge
adapters. It does not add a new adapter ABI, destination policy, or provider
integration.

## Authority and invariants

1. `wit/hydra-bridge.wit` remains the only legacy-CRM adapter ABI.
2. Adapter HTTP calls remain mediated by BridgeHost and the adapter's
   validated origin grant.
3. The Kernel must construct the existing `bridge_host::ReqwestEgressClient`
   with `HYDRA_EGRESS_PROXY_URL` when building a configured bridge lifecycle.
4. The Kernel must not use `DenyEgressClient` for a configured lifecycle. The
   deny client remains valid for standalone and deterministic test helpers.
5. Staging and production configuration already fail closed when the proxy is
   missing or malformed. Development and tests may omit it and use the direct
   compatibility constructor.
6. Proxy construction errors must not echo credentials or the full proxy URI.
7. Adapter grants, Wasmtime fuel, tenant-scoped KV, named secrets, and the
   Store-only SQL boundary remain unchanged.

## Runtime contract

When `HYDRA_ADAPTERS_PATH` is configured and the bridge secret source is
available, Kernel startup constructs:

```text
validated LlmRuntimeConfig.egress_proxy_url
  -> ReqwestEgressClient::new_with_proxy
  -> BridgeLifecycle
  -> BridgeHost HostState
  -> WIT host.http
```

The optional value is passed as `None` only in development/test-compatible
configurations. A configured proxy is used for every adapter HTTP request;
there is no ambient-proxy fallback.

## Failure modes

- Invalid proxy configuration fails Kernel configuration before runtime
  construction.
- A proxy-client construction error makes the configured bridge lifecycle
  unavailable and does not register lifecycle handlers.
- A missing adapter root or unavailable secret source remains unavailable.
- A denied origin remains a BridgeHost grant error.
- Adapter upstream errors remain typed bridge errors and do not expose
  credentials or raw proxy configuration.

## Acceptance

- The real Kernel lifecycle builder contains no unconditional
  `DenyEgressClient` path.
- The existing proxy-aware BridgeHost client is constructed from the typed
  configuration.
- A focused runtime test proves the configured bridge path is available with
  an explicit proxy setting and rejects malformed proxy construction.
- The egress policy checker proves both the client constructor and its real
  Kernel call site.
- Existing bridge lifecycle, security, dependency, preflight, state, and full
  verifier gates pass.
- No external provider, staging, production database, deployment, tag, or
  push is used as evidence.
