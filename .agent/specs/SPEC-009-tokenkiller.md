# SPEC-009 TOKENKILLER (TK) — Token Economics & Output Containment
Status: Accepted | Owner: djw | Phase: 3 | ExecPlans: EP-004 (core), EP-008 (telemetry) | ADR-0006

## User-visible goal
Agent features cost cents, not dollars: DeepSeek routes sustain ≥97% prefix-cache hits; no model output ever floods the system.

## Non-goals
Semantic prompt compression; embedding caches; provider-side batching; fine-tuning.

## Terms
Segment = independently serialized prompt block with stability class S0(constitution/system, changes ~never) S1(tool/output-contract schemas, per-release) S2(tenant policy snapshot, per-config-version) S3(dynamic tail: task, retrieved records, latest turns). PrefixSha = sha256 of bytes S0..S2. Block = 64-token cache unit (assumption A3). Nuke = output exceeding route budget or matching dump patterns.

## Required behavior
TK1 Canonicalization: `canon::to_bytes(value)` — JSON with lexicographically sorted keys, UTF-8 NFC, LF, no insignificant whitespace, floats via ryu shortest, ints plain; map iteration order NEVER leaks. Idempotent: canon(canon(x))==canon(x).
TK2 Assembly: `prefix::assemble(route, segments) -> Prompt` orders S0→S1→S2→S3, joins with single '\n', computes token count with the provider tokenizer adapter, PADS the S2/S3 boundary with '\n'-repeat so len(S0..S2) ≡ 0 mod 64 tokens (alignment keeps the stable region on block boundaries); forbids in S0–S2: timestamps, uuids, counters, request ids, unsorted maps, float formatting drift (checked by `debug_assert_stable()` which re-canonicalizes and compares sha).
TK3 Transcript discipline: Session stores turns append-only; a turn once sent is byte-frozen; summarization (when tail > route.tail_budget) MOVES old turns into a summarized S3-head block appended AFTER the frozen region boundary marker — never edits history bytes. (Cache math: only the changed suffix is re-billed.)
TK4 Routing: every call carries RouteCfg{provider prefs, max_tokens, output_budget_bytes, contract, pii}. PII ⇒ private-tag providers only (shared invariant INV-4; TK enforces too — defense in depth).
TK5 NukeGuard: wraps the response stream; aborts when any: bytes > budget, single line > 4KiB, ≥64 consecutive fenced-code lines, base64 run > 2KiB, or JSON depth > 32. Abort ⇒ error tk_output_nuked; ONE repair retry appends S3 tail block: "Your previous output exceeded the size contract. Return ONLY <contract summary>, max N bytes." Second nuke ⇒ fail. Increment tk_nuke_aborts_total.
TK6 Output contracts per route (contracts.rs): e.g. `EnvelopeProposal` (JSON, ≤2KiB, schema-checked), `UnifiedDiff` (≤8KiB, must start `--- `), `MappingYaml` (≤16KiB). Full-file dumps are contract violations even under budget. Contract failure ⇒ same repair-once path.
TK7 Ledger: per call record {route,provider,prefix_sha,hit_tokens,miss_tokens,out_tokens,out_bytes,aborted,cost_cents}; DeepSeek usage fields prompt_cache_hit_tokens/prompt_cache_miss_tokens are read via provider trait `CacheUsage`; non-caching providers record miss=all. Rolling 1h ratio per route exported as tk_cache_hit_ratio.
TK8 CI replay gate: `scripts/cache-hit-audit.sh` replays tests/fixtures/tk-corpus/ (≥40 realistic agent-loop calls across 3 routes) against the wiremock DeepSeek fake (real longest-prefix accounting) and fails unless overall ratio ≥ TK_HIT_RATIO_TARGET (0.97) and no call nuked.
TK9 Constitution coupling: month-to-date cost_cents from ledger feeds governor constitution; ≥cap ⇒ router degrades routes to local provider and pages.

## Inputs/Outputs
In: route name, segment values, tail turns. Out: validated contract value + LedgerRow. Errors per SPEC-006 (tk_*).

## Why ≥97% is achievable (design math, informative)
Agent-loop calls share S0(≈1.2k tok)+S1(≈1.5k)+S2(≈0.8k)=~3.5k stable vs S3 tail ≈100–150 tok/call after the first; steady-state miss ≈ tail-only ⇒ ratio ≈ 3500/3650 ≈ 0.959 at turn 2 and >0.97 from turn 3 as frozen history accrues into the hit region. Corpus mirrors this shape; if a route can't reach it structurally (one-shot cold calls), it must be tagged `tk_exempt` with ADR — exempt routes excluded from the SLO but still NukeGuarded.

## Required tests
unit: canon idempotence proptest; assemble alignment (len%64==0 via fake tokenizer); frozen-turn immutability; nukeguard trip table (each trigger); contract validators; ledger math. integration: deepseek fake ratio ≥0.97 on corpus; repair-once behavior; cap-degrade routing.

## Acceptance
`bash scripts/cache-hit-audit.sh` → `cache-hit audit: ok (ratio=0.9XX)`; unit+integration green; reference/tokenkiller/*.rs adapted into crates/tokenkiller with tests named tk_*.
