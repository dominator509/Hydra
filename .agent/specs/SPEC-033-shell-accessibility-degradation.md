# SPEC-033 Shell Accessibility and Degradation Contract
Status: Accepted | Owner: Hydra maintainers | Phase: 5 | ExecPlans: EP-053

## Goal
Make the server-rendered Shell's core navigation and mutation controls usable
with keyboard input and without JavaScript, while retaining htmx as optional
progressive enhancement.

## Normative requirements

1. The document must expose a skip link to the main content, a labeled
   application navigation landmark, a main landmark, and a contentinfo
   landmark.
2. Disclosure controls must use native `details`/`summary` semantics when the
   interaction is only showing or hiding server-rendered content. Core
   controls must not require an `onclick` handler to become usable.
3. Every enhanced mutation form must retain a real `action` and `method="post"`
   fallback. An htmx request may replace or target the response, but it must
   not be the only submission path.
4. Existing form labels, CSRF fields, focus-visible styles, and the select
   alternative for stage movement remain required.
5. Flash dismissal and other convenience behavior may remain enhanced, but
   its absence must not prevent reading or submitting core content.
6. The local contract gate must fail when the required semantic markers or
   native form fallbacks are removed.

## Explicit boundary

This specification provides source-level and rendered-template contract
evidence only. Browser keyboard traversal, screen-reader output, contrast
measurement, and staging review remain EP-010 production-readiness evidence.

## Compatibility

The Shell remains server-rendered, uses Askama and existing Axum routes, keeps
the vendored htmx asset, and introduces no Node/npm toolchain or browser test
dependency. Existing URLs and POST actions remain unchanged.

## Acceptance

`sh scripts/test-shell-accessibility.sh` prints `shell accessibility: ok`.
The focused test proves the required layout landmarks, native disclosures, and
native form fallbacks. `bash scripts/verify.sh` remains green.
