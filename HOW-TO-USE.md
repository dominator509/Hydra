# How to Use This Blueprint Pack

1. Place files into the repository. Copy the CONTENTS of this pack folder into your empty (or target) repo root so AGENTS.md, .agent/, scripts/, reference/ sit at top level. `git add -A && git commit -m "HYDRA blueprint pack"`. Then `chmod +x scripts/*.sh` if the bits didn't survive transport.

2. Choose the active ExecPlan. Exactly one plan is active at a time (.agent/EXECUTION_RULES.md rule 1). Order is EP-000 → EP-010. Greenfield repos still run EP-000 (it verifies the greenfield assumption and probes the toolchain). Record the active plan by simply telling the agent its path — the plan file itself carries all state (Progress/Decision Log).

3. Run preflight. `bash scripts/preflight.sh` → `preflight: ok`. Pre-EP-001 it will NOTE that the workspace isn't initialized; that's expected. Any ERROR line must be fixed before an agent starts.

4. Run a lower-tier coding LLM against an ExecPlan. Open .agent/prompts/execute-active-execplan.md, replace [EXECPLAN_PATH] (and optionally [OPTIONAL_USER_REQUEST]), and paste it into your coding agent — or use the generic invocation prompt below. The agent should end with the AGENTS.md §15 report.

5. Continue a partially completed plan. Use .agent/prompts/continue-execplan.md. It re-validates the last ticked milestone before resuming — ticks are trusted only after re-verification.

6. Debug failing validation. Use .agent/prompts/debug-validation-failure.md with the exact failing command. It enforces the bounded-retry ladder (smallest fix → narrower diagnostic → change approach + log) so agents don't spiral.

7. Perform final review. Use .agent/prompts/final-review.md. It re-runs verify, diffs changed files against the plan's Files to Change, re-executes acceptance commands, and fills Outcomes & Retrospective. Extra changed files without Decision Log justification = failed review.

8. Decide production readiness. Only via EP-010: drills D1–D5 logged in OPERATIONS.md with dates, PRODUCTION_READINESS.md evidence rows filled, then `bash scripts/production-readiness-check.sh` → `production readiness: ok`. The final Sign-off row and PROMOTE=yes are human-only.

9. Avoid roadmap-only implementation. ROADMAP.md sequences phases; it contains zero implementation authority. If an agent cites the roadmap as its instruction source, stop it and hand it the ExecPlan. (The rule is embedded in AGENTS.md, EXECUTION_RULES, and every prompt.)

10. Update plans as the repository evolves. Reality wins: when code/tests diverge from a plan, update the PLAN in the same commit (Decision Log entry), keep specs normative via ADRs for behavior changes, and bump S0–S2 TOKENKILLER segment versions deliberately (each bump is an intentional cache reset — watch tk_cache_hit_ratio after).

## Generic lower-tier coding LLM invocation prompt

Read AGENTS.md, COMMANDS.md, .agent/PLANS.md, and [EXECPLAN_PATH].
Implement [EXECPLAN_PATH] to completion.
Do not ask for next steps.
Do not implement from ROADMAP.md directly.
Do not broaden scope.
Complete milestones in order.
Validate after each milestone.
Update the ExecPlan as you work.
Use only commands from COMMANDS.md.
Copy difficult implementations from reference/ and adapt; do not re-derive them.
Stop only for STOP conditions in AGENTS.md.
At the end, run the required verification command, run git diff --name-only, update Outcomes & Retrospective, and report changed files, commands run, results, decisions, risks, and acceptance status.

## Codex-style example

codex --cd . \
  --ask-for-approval never \
  --sandbox workspace-write \
  "Read AGENTS.md, COMMANDS.md, .agent/PLANS.md, and .agent/execplans/EP-001-foundation.md. Implement EP-001-foundation.md to completion. Do not ask for next steps. Stop only for STOP conditions in AGENTS.md. Update the ExecPlan as you work. Run validation after each milestone."

If your runner does not support those flags, the same instruction can be pasted verbatim into any coding agent that can read files, edit files, and run terminal commands (Claude Code, IDE agents, terminal agents).

## TOKENKILLER quick orientation (why it's threaded everywhere)
Every LLM call goes agent → tokenkiller::Session → router (TK-1). Session assembles canonical, stability-ordered segments (S0 constitution → S1 tool schemas → S2 tenant policy → S3 tail) padded to DeepSeek's 64-token cache blocks, streams the response through NukeGuard (byte/line/fence/base64/depth budgets, abort + one repair retry), validates the route's output contract, and writes hit/miss tokens to the ledger. `bash scripts/cache-hit-audit.sh` is the CI gate that fails the build if the replay corpus drops below 0.97. When the ratio dips in prod, the prefix_sha column in tk_ledger tells you exactly which segment changed.
