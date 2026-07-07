# .agent/EXECUTION_RULES.md — Consolidated Rules for Lower-Tier Coding Agents

1. ONE ACTIVE EXECPLAN. Work only the plan you were given. Do not open another.
2. NO HIDDEN CONTEXT. Everything you need is in AGENTS.md, COMMANDS.md, the plan, specs, and the repo. If it is not written, read the repo; if still unknown, smallest reversible assumption + Decision Log entry.
3. NO ROADMAP-ONLY IMPLEMENTATION. ROADMAP.md is strategy. Implementing from it directly is a violation.
4. CONTINUE BY DEFAULT. Finish the plan start-to-end. Do not ask "should I proceed?".
5. STOP ONLY for AGENTS.md §4 conditions, with blocker/evidence/decision/default.
6. ANTI-DRIFT: only Files-to-Change; non-goals binding; no refactor safaris.
7. ANTI-HALLUCINATION: verify every symbol/command/env/table/route by reading files; copy hard parts from reference/; commands only from COMMANDS.md.
8. ANTI-FIXATION: bounded retry 1-smallest-fix / 2-narrower-diagnostic / 3-change-approach+log. Never delete failing tests.
9. TEST BEFORE COMPLETION: milestone validation each step; verify.sh at the end.
10. DIFF REVIEW: `git diff --name-only` ⊆ Expected Changed Files or justified in Decision Log.
11. FINAL RESPONSE: per AGENTS.md §15, always, even after STOP.
