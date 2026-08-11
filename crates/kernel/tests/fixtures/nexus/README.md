# Fake Nexus fixtures

The EP-015 harness generates isolated tenant, binding, request, objective, task, consumer, and event identifiers at test runtime. Its deterministic Ed25519 private key exists only as test DER bytes; only the corresponding public JWK is served by the loopback fake issuer.

The harness never contacts a live Nexus repository, identity provider, production database, or production NATS server. Run it only with the documented test `DATABASE_URL` and JetStream-enabled `NATS_URL`.
