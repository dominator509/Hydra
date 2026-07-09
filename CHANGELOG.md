# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Deployment & release pipeline (EP-009):
  - Multi-stage Dockerfile with non-root user and healthcheck
  - Full docker compose topology: Caddy TLS termination, egress proxy, one-shot migrate container
  - n8n community nodes: Hydra Trigger (webhook/polling) and Hydra Action (envelope proposal)
  - GitHub Actions release workflow: tag-driven build, push, and staging deploy
  - Promotion script with human gate (PROMOTE=yes) and tty confirmation
  - Changelog bootstrapped in Keep a Changelog format

## [0.1.0] — 2026-07-08

### Added

#### Core Architecture (EP-001)
- Rust workspace with multi-crate architecture
- Kernel binary (`hydra-kernel`) with Axum HTTP server
- Configuration validation system with environment variable parsing
- Postgres and NATS connectivity with health checks
- Repository structure: `crates/kernel`, `crates/store`, `crates/governor`, `crates/cdm`,
  `crates/shell`, `crates/fabric`, `crates/agents`, `crates/llm-router`, `crates/bridge-wit`,
  `crates/bridge-host`, `crates/tokenkiller`
- Verification pipeline: `scripts/verify.sh` with preflight, lint, format, typecheck, test, build,
  security, and dependency audit steps
- CI workflow (`.github/workflows/ci.yml`) with Postgres and NATS service containers

#### Domain Model & Schema (EP-002)
- Canonical Data Model (CDM) with kind-registry and JSON Schema validation
- Core domain types: Entity, Edge, Event, Envelope, Tenant
- SQLx migrations: entities, edges, event log, outbox, envelopes
- Stores: entity repository, edge repository, event repository, envelope repository
- Ephemeral test schema (`TestDb`) for isolated integration testing

#### Persistence & SQL Migration Runner (EP-003)
- Postgres-backed `Store` layer with all repositories
- Migration runner embedded via `sqlx::migrate!` compile-time macro
- Adapter KV store for adapter-scoped scratch state
- Autonomy cell storage for runtime-configurable behavior staging

#### Entity Routes & Bridge Host (EP-004)
- Fabric REST API: entity CRUD, autonomy management, bridge configuration
- Bridge host runtime: Wasmtime-based adapter sandbox with host interface
- Bridge WIT world definition (`hydra:bridge@1.0.0`)
- Bridge service layer: adapter lifecycle, HTTP egress via host, secret resolution
- Running adapters (`adapter-memcrm`) compiled to `wasm32-wasip2`

#### Autonomy & Governor (EP-005 — placeholder)
- Governor state machine with tiered escalation (L1-L3)
- StoreAgenda and StoreOverride repositories
- Domain error types for policy violations

#### TokenKiller Token Economics (EP-006)
- TokenKiller ledger: precision token cache with adaptive budget
- Hit-ratio targeting, output-budget enforcement
- `bridge` and `spend` endpoints for token-aware operations
- NATS relay for async ledger operations
- `tk_cache_hit_ratio` metrics endpoint

#### Multi-Protocol Shell (EP-008)
- Server-rendered shell UI with htmx
- Session-based authentication (login/logout)
- Concierge service for human-in-the-loop escalation
- Static asset serving (htmx.js)
- Security middleware: headers, rate limiting

[Unreleased]: https://github.com/Dominator509/Hydra/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Dominator509/Hydra/releases/tag/v0.1.0
