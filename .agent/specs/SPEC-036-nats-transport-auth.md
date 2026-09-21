# SPEC-036 NATS Transport Authentication
Status: Accepted | Owner: Hydra maintainers | Phase: 6 | ExecPlans: EP-056

## Goal

Make Hydra's NATS client boundary fail closed for staging and production
without weakening the plain loopback development and test profile.

## Normative requirements

1. Hydra reads NATS authentication and TLS settings from explicit environment
   variables and never logs credential contents.
2. `NATS_CREDS_FILE` is the only supported production authentication input in
   this plan. Credentials embedded in `NATS_URL` are rejected.
3. `NATS_REQUIRE_AUTH` and `NATS_TLS_REQUIRED` default to `true` in staging
   and production and must not be disabled there.
4. Staging and production require an existing credentials file and a TLS
   connection. A custom CA file and optional client certificate/key pair may
   be supplied for private deployments.
5. The running Kernel and the explicit event-replay command use the same
   verified async-NATS connection options.
6. Development and local test profiles remain compatible with plain
   loopback NATS when the secure flags and credentials file are omitted.
7. Readiness continues to report broker reachability, while configuration
   validation fails before serving if the secure transport contract is
   incomplete.
8. The checked-in Compose topology does not publish NATS. The Nexus example
   documents operator-mounted credentials and trust anchors rather than
   committing secrets or pretending that a local NATS process is a production
   broker.

## Environment contract

| Variable | Meaning |
|---|---|
| `NATS_CREDS_FILE` | Mounted NATS user credentials file; required in staging/prod |
| `NATS_REQUIRE_AUTH` | Require credentials; defaults true in staging/prod |
| `NATS_TLS_REQUIRED` | Require TLS; defaults true in staging/prod |
| `NATS_TLS_CA_FILE` | Optional PEM CA bundle for private NATS TLS |
| `NATS_TLS_CLIENT_CERT_FILE` | Optional mTLS client certificate |
| `NATS_TLS_CLIENT_KEY_FILE` | Optional mTLS client key; required with the certificate |

## Boundary

This specification hardens the Hydra client and configuration boundary. It
does not create a NATS account, rotate credentials, generate certificates,
change production infrastructure, or claim staging connectivity.
