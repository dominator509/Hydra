use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::services::AppState;

pub async fn openapi(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Hydra API",
            "version": "1.0.0",
            "description": "Standalone Hydra APIs plus the authenticated Nexus interoperability facade."
        },
        "security": [
            { "localHydraSession": [] },
            { "localHydraBearer": [] }
        ],
        "components": {
            "securitySchemes": {
                "localHydraSession": {
                    "type": "apiKey",
                    "in": "cookie",
                    "name": "hydra-session"
                },
                "localHydraBearer": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Hydra-local session token. The hydra-dev-admin token exists only when the kernel runs in development mode."
                },
                "nexusOidcBearer": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT",
                    "description": "Short-lived asymmetric token validated against the configured Nexus issuer and Hydra-owned business binding."
                }
            }
        },
        "paths": {
            "/v1/openapi.json": {
                "get": { "summary": "OpenAPI document", "security": [] }
            },
            "/.well-known/oauth-protected-resource": {
                "get": { "summary": "OAuth protected-resource metadata", "security": [] }
            },
            "/.well-known/oauth-protected-resource/mcp": {
                "get": { "summary": "MCP OAuth protected-resource metadata", "security": [] }
            },
            "/v1/autonomy/cells": {
                "get": { "summary": "List autonomy cells" },
                "put": { "summary": "Replace autonomy cells" }
            },
            "/v1/bridges": {
                "post": { "summary": "Register a bridge adapter via an envelope-gated request" }
            },
            "/v1/bridges/{id}/status": {
                "get": { "summary": "Read bridge deployment status" }
            },
            "/v1/bridges/{id}/pause": {
                "post": { "summary": "Pause bridge activity" }
            },
            "/v1/bridges/{id}/resume": {
                "post": { "summary": "Resume bridge activity" }
            },
            "/v1/entities/{kind}": {
                "get": { "summary": "List entities by kind" },
                "post": { "summary": "Create an entity through the trusted local-human API" }
            },
            "/v1/entities/{kind}/{id}": {
                "get": { "summary": "Get an entity" },
                "patch": { "summary": "Patch an entity via JSON Merge Patch" },
                "delete": { "summary": "Soft delete an entity" }
            },
            "/v1/envelopes": {
                "get": { "summary": "List envelopes by state" },
                "post": { "summary": "Propose an envelope" }
            },
            "/v1/envelopes/{id}/approve": {
                "post": { "summary": "Approve an envelope through the local-human API" }
            },
            "/v1/envelopes/{id}/reject": {
                "post": { "summary": "Reject an envelope through the local-human API" }
            },
            "/v1/tk/ledger": {
                "get": { "summary": "Read tenant-scoped TOKENKILLER ledger stats" }
            },
            "/v1/concierge/ping": {
                "post": { "summary": "Smoke-test the TOKENKILLER call path with a concierge ping" }
            },
            "/oauth/token": {
                "post": {
                    "summary": "Disabled compatibility endpoint",
                    "description": "Hydra is an OAuth resource server and does not issue Nexus access tokens. This endpoint returns 503.",
                    "security": []
                }
            },
            "/v1/nexus/capabilities": {
                "get": { "summary": "List canonical Hydra capabilities", "security": [{ "nexusOidcBearer": [] }] }
            },
            "/v1/nexus/context": {
                "get": { "summary": "Read compact deterministic CRM context", "security": [{ "nexusOidcBearer": [] }] }
            },
            "/v1/nexus/proposals/stage-change": {
                "post": {
                    "summary": "Propose an idempotent governed canonical deal stage change",
                    "description": "Returns an ActionEnvelope receipt; never mutates a CRM entity directly.",
                    "security": [{ "nexusOidcBearer": ["hydra.crm.propose"] }]
                }
            },
            "/v1/nexus/envelopes/{id}/approval": {
                "post": {
                    "summary": "Record a human-delegated approval decision for one pending envelope",
                    "security": [{ "nexusOidcBearer": ["hydra.envelopes.approve"] }]
                }
            },
            "/v1/nexus/bindings": {
                "get": { "summary": "Read the caller's resolved active binding", "security": [{ "nexusOidcBearer": [] }] }
            },
            "/v1/nexus/events/status": {
                "get": { "summary": "Read truthful Nexus event-contract availability", "security": [{ "nexusOidcBearer": [] }] }
            },
            "/mcp": {
                "get": {
                    "summary": "MCP Streamable HTTP SSE or explicit method-not-allowed response",
                    "security": [{ "nexusOidcBearer": [] }]
                },
                "post": {
                    "summary": "MCP 2025-11-25 Streamable HTTP JSON-RPC endpoint",
                    "security": [{ "nexusOidcBearer": [] }]
                }
            }
        },
        "x-hydra-capabilities": state.capabilities.descriptors()
    }))
}
