# EP-XXX <Title>

## 1. Purpose / Big Picture
<Why this exists; what the system can do after that it couldn't before.>

## 2. Scope
<Exact components/dirs in scope.>

## 3. Non-goals
<Binding exclusions. Anything here appearing in the diff fails review.>

## 4. Context and Orientation
<State of the repo this plan assumes; pointers to normative specs; reference/ files to copy-adapt.>

## 5. Files to Read First
<Exact paths.>

## 6. Files to Change (Expected Changed Files)
<Exact paths, new files marked (new).>

## 7. Interfaces and Contracts
<Public APIs/routes/schemas this plan creates or must not break — exact names.>

## 8. Milestones
M1 <goal>. Read: <paths>. Change: <paths>. Exact edits: <what>. Validation: `<command>`. Expected: `<output>`. Recovery: <instruction>.
M2 ... (repeat; every milestone MUST have validation command + expected result + recovery)

## 9. Concrete Steps
<Ordered actions incl. service prerequisites and commit-message convention `EP-XXX: M<n> <slug>`.>

## 10. Validation and Acceptance
<Final acceptance commands + expected outputs; always ends with `bash scripts/verify.sh` → `verify: ok` and diff-vs-§6 review.>

## 11. Idempotence and Recovery
<How to resume after interruption at any milestone; what is safe to rerun.>

## 12. Progress
- [ ] M1  - [ ] M2  - [ ] ...

## 13. Surprises & Discoveries
<Append during execution: unexpected findings, failed hypotheses (anti-fixation rule 3).>

## 14. Decision Log
<Dated entries: context → decision → why smallest/reversible.>

## 15. Outcomes & Retrospective
<Filled at completion: shipped, deviations, risks, follow-ups.>
