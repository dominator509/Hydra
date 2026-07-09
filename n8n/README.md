# n8n-nodes-hydra

n8n community nodes for integrating with [Hydra](https://github.com/Dominator509/Hydra) — a Rust-powered,
Wasm-sandboxed integration runtime.

## Installation

### Prerequisites

- [n8n](https://n8n.io/) >= 1.0.0
- Node.js >= 18.10
- pnpm >= 9.1

### Install via n8n community nodes

```bash
# From your n8n installation directory
npm install n8n-nodes-hydra
```

### Manual installation (development)

```bash
git clone <repo-url>
cd n8n
pnpm install
pnpm build
pnpm link --global
# In your n8n directory:
pnpm link --global n8n-nodes-hydra
```

## Configuration

Set the following environment variables in your n8n instance, or configure per-node:

| Variable           | Description                        | Default |
|--------------------|------------------------------------|---------|
| `HYDRA_BASE_URL`   | Base URL of the Hydra instance     | —       |
| `HYDRA_AUTH_TOKEN` | Hydra API authentication token     | —       |

### Per-node configuration

Each node also accepts `Hydra Base URL` and `Auth Token` as node parameters, which
override the environment variables.

## Nodes

### Hydra Trigger (webhook)

Subscribes to Hydra webhook events at the endpoint:

```
POST /v1/webhooks/{tenant}/{event}
```

**Modes:**
- **Webhook** (default): Hydra pushes events to this node when they occur.
  Register the webhook URL in your Hydra tenant configuration.
- **Polling**: Periodically polls `/v1/events/pending` for new events.

**Output:** A JSON object containing the event payload, tenant, and event type.

### Hydra Action

Proposes an envelope to the Hydra REST API at:

```
POST /v1/envelopes/propose
```

**Parameters:**
| Field      | Type   | Description                                      |
|------------|--------|--------------------------------------------------|
| `domain`   | string | The Hydra domain (e.g., `crm`, `support`)        |
| `action`   | string | Action to perform (`upsert`, `delete`, `sync`)   |
| `targets`  | array  | Target kind names (e.g., `["contacts"]`)         |
| `payload`  | object | JSON envelope payload                            |
| `rationale`| string | Human-readable explanation for audit             |

**Output:** The created envelope ID and current state.

## Usage Example

1. Add a **Hydra Trigger** node (webhook mode, tenant: `acme-corp`, event: `envelope.created`)
2. Connect a **Hydra Action** node
3. Configure the action to process the incoming event:
   - Domain: `crm`
   - Action: `upsert`
   - Targets: `["contacts"]`
   - Payload: `{{ $json.payload }}`
   - Rationale: `"Processed from n8n trigger"`
4. Deploy the workflow

## Compatibility

| Version | n8n Version | Hydra Version |
|---------|-------------|---------------|
| 0.1.0   | >= 1.0.0    | >= 0.1.0      |

## License

MIT
