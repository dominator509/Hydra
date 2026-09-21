# SPEC-027 Wasmtime Feature Boundary

Status: Accepted

## Purpose

Define the minimum explicit Wasmtime feature boundary Hydra needs for its
Wasmtime/WIT legacy-CRM adapter ABI while preventing optional profiling
features from entering the production dependency graph unintentionally.

## Normative Requirements

1. The workspace `wasmtime` dependency MUST remain pinned to the reviewed
   version and MUST set `default-features = false`.
2. The direct Wasmtime feature set MUST explicitly include the features needed
   by `crates/bridge-host`: asynchronous component execution, the Component
   Model, the runtime, the standard library support, and Cranelift compilation.
3. `wasmtime-wasi` MUST remain pinned to the same reviewed version and MUST
   continue to provide the WASI implementation required by the checked-in
   adapter components.
4. Hydra MUST NOT enable Wasmtime profiling, cache, GC, coredump, or other
   optional defaults unless a later ADR proves the runtime need and reviews
   the resulting dependency graph.
5. The all-target locked dependency graph MUST contain no `fxhash` package
   pulled solely by Wasmtime's optional profiling path.
6. The change MUST preserve the WIT ABI, adapter grants, tenant isolation,
   Wasmtime sandboxing, and component conformance behavior.
7. The unmaintained `proc-macro-error2` edge pulled by the pinned `age`
   release MUST remain visible to audit and documented as an upstream
   residual unless a compatible age release removes it; this specification
   does not authorize replacing the cryptographic vault library.
8. Dependency, security, preflight, focused bridge, and full verification
   gates MUST pass without advisory ignores or masked failures.

## Non-goals

- Upgrading Wasmtime or age.
- Disabling Wasmtime, Cranelift, WASI, or the legacy adapter ABI.
- Adding a second adapter runtime or changing WIT.
- Suppressing audit output for unmaintained transitive packages.
- Claiming staging or production supply-chain approval from local builds.
