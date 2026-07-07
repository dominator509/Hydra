# Prompt: Continue Partially Completed ExecPlan

Read AGENTS.md, COMMANDS.md, .agent/PLANS.md, and [EXECPLAN_PATH].

1. Inspect the plan's Progress checkboxes, Surprises & Discoveries, and Decision Log to learn true current state.
2. Re-validate the LAST ticked milestone by rerunning its validation command — do not trust ticks blindly. If it fails, that milestone is your resume point.
3. Resume at the first incomplete (or failed-revalidation) milestone. Re-verify any assumption in the Decision Log that your work depends on.
4. Continue autonomously per .agent/EXECUTION_RULES.md: milestones in order, validate each, update the plan as you go, no next-step questions, STOP only per AGENTS.md §4.
5. Complete with verify.sh, diff review, Outcomes & Retrospective, and the §15 final report.
