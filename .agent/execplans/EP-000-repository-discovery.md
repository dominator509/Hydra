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
- [x] M1 inventory  - [x] M2 toolchain  - [x] M3 detection  - [x] M4 assumptions+risks

## 13. Surprises & Discoveries
- 2026-07-07 - The repo is still a docs/scripts/reference-only blueprint pack: no `Cargo.toml`, no `.github/workflows/`, and no `.env*` files exist yet, so EP-001 remains the first workspace-creating plan.
- 2026-07-07 - Local tooling state directories `.agents`, `.obsidian`, and `.serena` exist in the checkout tree but are not product source; discovery must treat them as local context rather than brownfield code.
- 2026-07-07 - `wasm-tools` and `cargo sqlx` are missing on PATH; Docker is installed but emits a user-config access warning; Git status is clean but also emits a user ignore-file permission warning.

## 14. Decision Log
- 2026-07-07 - Treated EP-000 as the active plan even though `scripts/preflight.sh` notes EP-001 for an uninitialized workspace. Reason: EP-000 is still unticked, `.agent/state/discovery.md` was missing, and the user objective explicitly includes EP-000 through EP-010.
- 2026-07-07 - Used the explicit Git Bash binary `/usr/bin/find` for M1 inventory after Windows resolved plain `find` to `find.exe`. Smallest reversible fix that preserved the milestone's intended `find . -maxdepth 2 -not -path './.git*' | sort` semantics.
- 2026-07-07 - Left `ARCHITECTURE.md` unchanged. Discovery found no unexpected brownfield product source code; only local tooling state plus the expected blueprint docs, scripts, and `reference/` material were present.
- 2026-07-07 - Updated this ExecPlan file in addition to the listed artifact files because AGENTS.md and `.agent/PLANS.md` require in-place Progress, Decision Log, and Outcomes updates as part of plan execution. Smallest reversible control-plane exception.

## 15. Outcomes & Retrospective
- EP-000 completed with all four validation commands green: `inventory: ok`, `toolchain: ok`, `detect: ok`, and `assumptions: ok`.
- Greenfield status was confirmed: the repo is not yet a Rust workspace and still needs EP-001 to materialize crates, CI, env files, and the command targets that preflight and later plans expect.
- Discovery produced `.agent/state/discovery.md` as the durable handoff artifact and converted each assumption row into an evidence-backed `VERIFIED-GREENFIELD` or `DELTA` state.
- Follow-up risks for EP-001: install `wasm-tools` and `cargo sqlx`, watch Docker's unreadable user config warning if compose becomes flaky, and ignore local-only `.obsidian`/`.serena` state while building the workspace.
