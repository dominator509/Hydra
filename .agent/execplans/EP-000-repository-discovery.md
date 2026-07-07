# EP-000 Repository Discovery

## 1. Purpose / Big Picture
Establish ground truth about the repository before any code: confirm greenfield status (or inventory what exists), verify toolchain availability, and update COMMANDS/ARCHITECTURE/ASSUMPTIONS with evidence. Mandatory first plan.

## 2. Scope
Read-only inspection + doc updates + creation of `.agent/state/discovery.md` report.

## 3. Non-goals
No source code, no Cargo.toml, no dependencies, no formatting of existing files.

## 4. Context and Orientation
This pack was authored for a greenfield repo (ASSUMPTIONS A1–A8). Discovery either confirms that or converts this pack to brownfield mode by recording deltas.

## 5. Files to Read First
AGENTS.md, COMMANDS.md, ARCHITECTURE.md, ASSUMPTIONS.md, .agent/EXECUTION_RULES.md.

## 6. Files to Change (Expected Changed Files)
.agent/state/discovery.md (new), ASSUMPTIONS.md (verify column), COMMANDS.md (only if evidence demands), ARCHITECTURE.md (only "current state" note).

## 7. Interfaces and Contracts
Output contract: discovery.md sections = {tree, toolchain versions, git status, stack detection, CI detection, env detection, risks, missing info}.

## 8. Milestones
M1 Inventory. Goal: capture repo tree + git state. Read: n/a. Change: .agent/state/discovery.md. Edits: paste outputs of `git status --porcelain`, `git log --oneline -5 || true`, `find . -maxdepth 2 -not -path './.git*' | sort`. Validation: `test -s .agent/state/discovery.md && echo inventory: ok`. Expected: `inventory: ok`. Recovery: recreate file; commands are read-only, rerun freely.
M2 Toolchain probe. Goal: confirm required tools per ENVIRONMENT.md. Change: discovery.md (+table). Edits: record `rustc --version; cargo --version; docker --version; docker compose version; jq --version; rg --version; wasm-tools --version || echo MISSING; cargo sqlx --version || echo MISSING; cargo audit --version || echo MISSING; cargo deny --version || echo MISSING`. Validation: `grep -c 'version\|MISSING' .agent/state/discovery.md` ≥ 8 → echo `toolchain: ok`. Expected: `toolchain: ok`. Recovery: rerun probes; MISSING entries are handled by EP-001 install, not here.
M3 Stack/CI/env detection. Goal: detect any pre-existing package manager, tests, CI, .env. Edits: record results of `ls -a; ls .github/workflows 2>/dev/null; cat .env* 2>/dev/null | sed 's/=.*/=REDACTED/'`. Validation: `grep -q 'CI:' .agent/state/discovery.md && echo detect: ok`. Expected: `detect: ok`. Recovery: rerun.
M4 Risk + assumption verification. Goal: mark each ASSUMPTIONS row's "How to verify" as VERIFIED-GREENFIELD / DELTA with one-line evidence; list risks (missing tools, dirty git, unexpected files). Change: ASSUMPTIONS.md, discovery.md. Validation: `grep -c 'VERIFIED\|DELTA' ASSUMPTIONS.md` ≥ 8 → echo `assumptions: ok`. Expected: `assumptions: ok`. Recovery: table edit is idempotent.

## 9. Concrete Steps
Exactly the milestone commands, from repo root, in order. Do not install or fix anything in this plan.

## 10. Validation and Acceptance
All four expected outputs seen; discovery.md complete; if ANY unexpected source code found, STOP-adjacent rule: switch ARCHITECTURE.md status note to brownfield and record required plan amendments in Decision Log before proceeding to EP-001 (this is a plan-update, not a STOP).

## 11. Idempotence and Recovery
Everything read-only or overwrite-safe; rerun any milestone freely; resume = check which sections exist in discovery.md.

## 12. Progress
- [ ] M1 inventory  - [ ] M2 toolchain  - [ ] M3 detection  - [ ] M4 assumptions+risks

## 13. Surprises & Discoveries
(append findings)

## 14. Decision Log
(append)

## 15. Outcomes & Retrospective
(fill at completion)
