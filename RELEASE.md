# RELEASE.md

Types: patch (fix), minor (feature), major (ABI/schema-visible). Versioning: SemVer git tags `vX.Y.Z`; adapter WIT world is versioned independently (`hydra:bridge@MAJOR.MINOR.PATCH`) — WIT major bump = HYDRA major bump.
Changelog: CHANGELOG.md, Keep-a-Changelog format, updated in the release PR.
Branching: trunk-based; `main` always releasable; release = tag on main.
RC criteria: verify.sh green on the tag commit; staging soak 24h with zero Sev1/2; cache-hit-audit ≥0.97 on staging corpus.
Checklist: .agent/checklists/release.md. Smoke: scripts/smoke-test.sh on staging then prod. Approvals: human `PROMOTE=yes` for prod (agents: STOP).
Release notes: user-visible changes + autonomy-default changes explicitly flagged (tenants must know if any cell default moved).
Post-release: 60-min watch on golden + TOKENKILLER dashboards; file retro notes in DECISIONS.md if any alert fired.
