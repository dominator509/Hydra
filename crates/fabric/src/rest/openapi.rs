use axum::Json;
use serde_json::{json, Value};

pub async fn openapi() -> Json<Value> {
    Json(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Hydra API",
            "version": "1.0.0"
        },
        "paths": {
            "/v1/openapi.json": {
                "get": { "summary": "OpenAPI document" }
            },
            "/v1/entities/{kind}": {
                "get": { "summary": "List entities by kind" },
                "post": { "summary": "Create an entity" }
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
                "post": { "summary": "Approve an envelope" }
            },
            "/v1/envelopes/{id}/reject": {
                "post": { "summary": "Reject an envelope" }
            },
            "/v1/tk/ledger": {
                "get": { "summary": "TOKENKILLER ledger stats" }
            }
        }
    }))
}
