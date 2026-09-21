# SPEC-028 Age Vault Dependency Boundary

Status: Accepted

## Purpose

Keep Hydra's encrypted named-secret vault on the maintained compatible age
release while removing the resolved `proc-macro-error2` maintenance warning
from its transitive localization macro path.

## Normative Requirements

1. The workspace `age` dependency MUST be pinned exactly to `0.12.1` for
   this change and MUST retain its default feature set as explicitly empty.
2. The resolved graph MUST use `i18n-embed-fl 0.10.1` or newer within the
   age 0.12.1 compatibility range and MUST not contain `proc-macro-error2`.
3. The age artifact format, passphrase behavior, named-secret document
   contract, key rotation, backup, restore, and owner-only output boundary
   MUST remain unchanged.
4. Hydra MUST NOT replace age with custom cryptography, an HTTP secret store,
   or an ambient provider.
5. `cargo audit`, `cargo deny check`, the vault/BridgeHost tests, and the full
   verifier MUST pass without advisory ignores or masked failures.
6. The result MUST be described as local dependency evidence only; owner-key
   custody, off-box protection, staged restore, and EP-010 human evidence
   remain separate requirements.

## Non-goals

- No vault format migration or re-encryption of existing artifacts.
- No change to `HYDRA_VAULT_KEY`, `HYDRA_VAULT_PATH`, or recovery commands.
- No production deployment, tag, push, registry action, or production DB use.
- No broad dependency refresh outside the age transitive graph.
