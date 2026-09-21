#!/usr/bin/env sh
# Validate the checked-in NATS transport contract without contacting a broker.
set -eu

fail() {
  echo "nats policy ERROR: $1" >&2
  exit 1
}

[ -f crates/kernel/src/nats.rs ] || fail "typed NATS transport module is missing"
[ -f .agent/specs/SPEC-036-nats-transport-auth.md ] || fail "SPEC-036 is missing"

grep -Fq -- "NATS_CREDS_FILE" crates/kernel/src/nats.rs || fail "credential-file configuration is missing"
grep -Fq -- "NATS_TLS_REQUIRED" crates/kernel/src/nats.rs || fail "TLS-required configuration is missing"
grep -Fq -- "require_tls" crates/kernel/src/nats.rs || fail "NATS connection does not require TLS"
grep -Fq -- "credentials_file" crates/kernel/src/nats.rs || fail "NATS connection does not load credentials files"
grep -Fq -- "NATS_CREDS_FILE" docker/compose.yaml || fail "Compose does not pass the NATS credential path"
grep -Fq -- 'NATS_URL: "${NATS_URL:-nats://nats:4222}"' docker/compose.yaml || fail "Compose does not permit an explicit NATS URL override"

for name in \
  NATS_CREDS_FILE \
  NATS_REQUIRE_AUTH \
  NATS_TLS_REQUIRED \
  NATS_TLS_CA_FILE \
  NATS_TLS_CLIENT_CERT_FILE \
  NATS_TLS_CLIENT_KEY_FILE
do
  count="$(awk -v name="$name" '$0 ~ "^[[:space:]]+" name ":" { count++ } END { print count + 0 }' docker/compose.yaml)"
  [ "$count" -eq 1 ] || fail "Compose must define $name exactly once"
done

grep -Fq -- "NATS_REQUIRE_AUTH" docker/nexus.env.example || fail "Nexus example does not declare NATS auth"
grep -Fq -- "NATS_TLS_REQUIRED" docker/nexus.env.example || fail "Nexus example does not declare NATS TLS"
grep -Eq '^NATS_URL=tls://[^[:space:]]+$' docker/nexus.env.example || fail "Nexus example does not require an operator TLS NATS URL"
grep -Fq -- "NATS_CREDS_FILE" docker/nexus.env.example || fail "Nexus example does not declare mounted credentials"
grep -Fq -- "check-nats-policy" scripts/preflight.sh || fail "preflight does not require the NATS policy gate"
grep -Fq -- "check-nats-policy.sh" scripts/verify.sh || fail "verify does not run the NATS policy gate"

if grep -Eq 'nats://[^[:space:]]+:[^[:space:]]+@' .env.example docker/nexus.env.example docker/compose.yaml; then
  fail "checked-in NATS examples embed credentials in a URL"
fi

echo "nats policy: ok"
