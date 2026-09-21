# ROLLBACK.md

Triggers: Sev1; smoke fail post-deploy; error rate >2% 10m; tk hit-ratio <0.90 caused by release; data-integrity anomaly.
Decision owner: operator on call (djw). Agents never roll back prod autonomously (STOP condition).
Types: (a) app image rollback (default), (b) config rollback (compose env / autonomy matrix / TK segment version), (c) DB rollback (rare; additive migrations mean usually none), (d) feature-flag style: autonomy cells can be dropped to L1 instantly (`hydra autonomy set <cell> L1`) as a functional rollback of agent behavior.
App rollback: set `.env` `HYDRA_TAG` to the prior immutable release version or
digest and run `docker compose up -d kernel`; verify smoke. The release
workflow does not publish a mutable Hydra `latest` alias.
DB: never down-migrate in prod; if a migration must be neutralized, write a new forward migration; data repair via scripted, transactional, reviewed SQL only.
Config: git-revert docker/.env or wiring/autonomy files; kernel hot-reloads signed config.
Verification: smoke test + 15-min dashboards. Communication: shell banner + status note. Postmortem: within 48h, template in .agent/templates/runbook-template.md appendix, filed in DECISIONS.md.
