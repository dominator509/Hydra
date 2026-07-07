# Checklist: Incident Response
- [ ] DETECT: alert/report captured verbatim (time, alert name, dashboards screenshot/values).
- [ ] TRIAGE: severity (Sev1 integrity/security; Sev2 feature down; Sev3 degraded); scope (tenants affected).
- [ ] MITIGATE first, diagnose second: options in order — autonomy cells → L1 freeze; pause bridge; route LLM to local provider; image rollback.
- [ ] COMMUNICATE: banner + status note; update every 30 min Sev1/2.
- [ ] RESOLVE: fix or rollback per checklists; capture commands run.
- [ ] VERIFY: smoke ok; alert cleared; 15-min watch.
- [ ] DOCUMENT: timeline, root cause hypothesis, evidence in DECISIONS.md.
- [ ] FOLLOW UP: postmortem ≤48h; regression test `regress_<incident>`; runbook updated.
