# SPEC-011: Optional Nexus Model, A2A, and Skill Trust Boundary

Status: Accepted for EP-016
Version: 1.0

## 1. Purpose

Define the optional post-v1 interoperability features that let Hydra use a
Nexus model gateway, expose bounded long-running workflows through A2A, and
advertise trusted Agent Skills without bypassing TOKENKILLER, Hydra Governor,
tenant isolation, or standalone operation.

This specification is additive. Hydra remains independently deployable and
continues to function when every optional Nexus feature is disabled.

## 2. Normative References

- A2A 1.0.0 protocol specification and JSON-RPC binding:
  `https://github.com/a2aproject/A2A/blob/main/docs/specification.md`
- Agent Skills format specification:
  `https://github.com/agentskills/agentskills/blob/main/docs/specification.mdx`
- Hydra interoperability contract: `.agent/specs/SPEC-010-nexus-interoperability.md`
- Hydra TOKENKILLER contract: `.agent/specs/SPEC-009-tokenkiller.md`

The Agent Skills format does not define a cryptographic trust or execution
protocol. Hydra therefore defines the signed manifest extension in section 6;
it is a Hydra trust policy, not a claim that the upstream skill format itself
provides signatures.

## 3. Non-goals

- No direct dependency on the Nexus source repository.
- No model call outside `tokenkiller::Session` and the Hydra LLM router.
- No LLM output is an approval, execute token, or Store mutation.
- No A2A operation for ordinary CRM reads or basic CRM mutations.
- No generic execute-anything workflow or provider passthrough.
- No bridge lifecycle capability is advertised until a real governed handler exists.
- No execution of skill scripts, shell commands, credentials, or tool grants from
  a skill package.
- No production deployment, live Nexus credential, or cloud identity dependency.

## 4. Nexus Model Gateway

### 4.1 Call path

The only supported path is:

`Hydra agent -> TOKENKILLER Session -> Hydra llm-router -> optional Nexus Model Gateway`

The gateway adapter implements the existing provider-neutral `LlmProvider`
trait and uses an OpenAI-compatible `POST /chat/completions` contract. It is
identified as provider `nexus` and is disabled unless an explicit URL and model
are configured. The adapter must not be reachable from `agents` directly.

### 4.2 Privacy and provenance

- A Nexus provider is `private` only when explicitly configured as private.
- PII routes reject it when that declaration is absent.
- Every response carries provider, model, gateway, privacy class, requested
  token budget, output byte budget, actual output tokens, and cost provenance.
- TOKENKILLER owns prompt assembly, stability checks, NukeGuard, output
  contracts, retry repair, and ledger recording before the response returns.
- Provider errors are redacted and must not include tokens, prompts, or full
  customer records.
- Local providers remain the fallback chain. If the optional provider is
  unavailable, Hydra reports the failure and continues only through configured
  providers; it never silently changes privacy class.

### 4.3 Secret source

The gateway token is read by Kernel from the encrypted named-secret vault
under the configured secret name. Raw tokens are not stored in routes,
durable provenance, logs, events, or capability responses.

## 5. A2A Workflow Facade

### 5.1 Protocol surface

Hydra exposes an authenticated A2A 1.0 JSON-RPC HTTP endpoint at `/a2a` and
the public Agent Card at `/.well-known/agent-card.json`. The endpoint supports
only these methods:

- `SendMessage`
- `GetTask`
- `ListTasks`
- `CancelTask`

The Agent Card declares JSON-RPC 1.0, `streaming: false`,
`pushNotifications: false`, and no extended card. `SendStreamingMessage`,
subscription, and push-notification methods return the standard unsupported
operation error rather than pretending to stream.

Requests require the existing authenticated Nexus principal and a Hydra
scope appropriate to the workflow. Task reads, cancellation, and writes are
tenant-scoped to the authenticated external binding. Missing, cross-tenant,
or unknown tasks return the same not-found error shape.

### 5.2 Allowlist

The only workflow names accepted by the facade are:

- `migration-assessment`
- `legacy-crm-discovery`
- `bridge-synthesis`
- `bridge-conformance`
- `bridge-wiring`
- `bridge-canary`

`migration-assessment` is a deterministic, read-only bounded workflow. The
remaining workflows are represented in discovery with their actual
unavailable reason until their typed runtime handlers exist. No A2A message
can construct an arbitrary ActionEnvelope or invoke a vendor API.

### 5.3 Durable task contract

- Tasks are persisted by Store in an additive tenant-scoped table.
- `messageId` is the idempotency key within a Hydra tenant.
- Reusing a message ID with a different request hash returns a deterministic
  conflict; equivalent reuse returns the original task.
- Task state transitions are compare-and-swap protected and append a bounded
  status history in the task document.
- Supported tasks may resume from `submitted` or `working` after restart.
- Cancellation is idempotent for non-terminal tasks and fails with the A2A
  task-not-cancelable error for terminal tasks.
- Durable task input contains only validated structured metadata; raw message
  text, tokens, prompts, secrets, and customer bodies are not persisted.

### 5.4 A2A data rules

JSON uses the A2A camelCase field names and ProtoJSON enum names such as
`TASK_STATE_WORKING`. Errors use JSON-RPC 2.0 codes and A2A codes
`-32001` through `-32009` where applicable. Timestamps are UTC ISO-8601.
Artifacts contain bounded structured results and no secret-shaped data.

## 6. Signed Agent Skills

### 6.1 Package

Each skill is a directory whose `SKILL.md` follows the upstream format. Hydra
adds a sibling `hydra-skill.json` manifest containing:

- schema version
- skill name and semantic version
- signer and key ID
- SHA-256 of the exact `SKILL.md` bytes
- compact Ed25519 JWS signature over the manifest claims
- declared least-authority scope and capability names

The signature claims must match the package directory, metadata name/version,
and content hash. Unknown algorithms, signers, key IDs, expired claims, hash
mismatches, malformed frontmatter, duplicate names, and invalid versions fail
closed.

### 6.2 Trust and execution

- Hydra loads public trust anchors from an owner-controlled configuration.
- The registry is deterministic and rejects duplicate skill name/version pairs.
- A skill may declare only scopes and capabilities already allowed by Hydra's
  explicit registry policy.
- A skill cannot grant credentials, add tools, approve envelopes, or change
  Governor policy.
- Discovery returns signed, versioned metadata and trust state. It never
  executes `scripts/` or returns raw instruction bodies by default.
- Untrusted or unavailable skills are omitted from executable discovery and
  reported with a safe reason in diagnostics.
- Kernel loads the registry only when `HYDRA_SKILLS_PATH` and
  `HYDRA_SKILLS_TRUST_FILE` are both present. The runtime inventory reports
  signed discovery disabled when both are absent, unavailable when configured
  but no package verifies, and available only when at least one package is
  trusted. No public path executes a skill package in this version.

## 7. Configuration

Optional configuration is fail-closed and documented in `ENVIRONMENT.md`:

- `NEXUS_MODEL_GATEWAY_URL`
- `NEXUS_MODEL_GATEWAY_MODEL`
- `NEXUS_MODEL_GATEWAY_TOKEN_SECRET`
- `HYDRA_SKILLS_PATH`
- `HYDRA_SKILLS_TRUST_FILE`

The first three configure the optional provider; the token value is retrieved
from the named vault secret. The last two enable signed-skill discovery. A
partial provider or skill configuration is invalid rather than silently
falling back to an unsafe mode.

## 8. Acceptance

EP-016 is complete only when:

- Nexus provider tests prove TOKENKILLER-only access, privacy blocking,
  provenance, budgets, fallback, and redacted failures.
- A2A tests prove protocol/version validation, allowlisting, tenant scoping,
  durable idempotency, cancellation, resume, unavailable capability truth,
  and no direct CRUD/mutation path.
- Skill tests prove official metadata validation, Ed25519 signature/hash
  verification, key rotation/trust failure, version conflict, and
  least-authority enforcement.
- `cargo deny check`, `cargo audit`, security checks, and `bash scripts/verify.sh`
  pass with no masked failures.
- Standalone Hydra remains functional with all optional configuration absent.
