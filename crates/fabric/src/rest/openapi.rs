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
