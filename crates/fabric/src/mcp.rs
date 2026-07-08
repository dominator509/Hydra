use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use governor::{EnvelopeState, Reversal};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::services::{
    AppState as FabricAppState, AutonomyService, BlastRadiusDto, BridgeService,
    ConciergeService, EnvelopeCreateRequest, EnvelopeService, EntityService, TkStatsService,
};

/// Default dev-mode tenant UUID for MCP requests without _meta.x-hydra-tenant.
const DEV_TENANT: &str = "00000000-0000-0000-0000-000000000001";

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 wire types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Serialize)]
pub struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcErrorBody>,
}

#[derive(Serialize)]
struct JsonRpcErrorBody {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

fn rpc_success(id: Option<Value>, result: Value) -> JsonRpcResponse {
    JsonRpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None }
}

fn rpc_error(id: Option<Value>, code: i32, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcErrorBody { code, message, data: None }),
    }
}

/// Build an MCP-format content result from a serializable value.
fn mcp_text_result(value: impl Serialize) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|e| format!("serialization error: {e}"));
    json!({ "content": [{ "type": "text", "text": text }] })
}

/// Extract tenant from arguments._meta.x-hydra-tenant, falling back to the dev default.
fn extract_tenant(arguments: Option<&Value>) -> Uuid {
    arguments
        .and_then(|a| a.get("_meta"))
        .and_then(|m| m.get("x-hydra-tenant"))
        .and_then(|t| t.as_str())
        .and_then(|t| Uuid::parse_str(t).ok())
        .unwrap_or_else(|| Uuid::parse_str(DEV_TENANT).expect("DEV_TENANT must be valid UUID"))
}

// ---------------------------------------------------------------------------
// MCP Handler
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct McpHandler {
    entities: Arc<dyn EntityService>,
    envelopes: Arc<dyn EnvelopeService>,
    autonomy: Arc<dyn AutonomyService>,
    bridges: Arc<dyn BridgeService>,
    concierge: Arc<dyn ConciergeService>,
    tk_stats: Arc<dyn TkStatsService>,
}

impl McpHandler {
    pub fn new(state: &FabricAppState) -> Self {
        Self {
            entities: state.entities.clone(),
            envelopes: state.envelopes.clone(),
            autonomy: state.autonomy.clone(),
            bridges: state.bridges.clone(),
            concierge: state.concierge.clone(),
            tk_stats: state.tk_stats.clone(),
        }
    }

    /// Dispatch a JSON-RPC request to the appropriate handler.
    async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id;
        match request.method.as_str() {
            "initialize" => self.handle_initialize(id),
            "tools/list" => self.handle_tools_list(id),
            "tools/call" => self.handle_tools_call(id, request.params).await,
            _ => rpc_error(id, -32601, format!("Method not found: {}", request.method)),
        }
    }

    // -- built-in MCP methods --

    fn handle_initialize(&self, id: Option<Value>) -> JsonRpcResponse {
        rpc_success(
            id,
            json!({
                "protocolVersion": "1.0",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "hydra-mcp", "version": "0.1.0" }
            }),
        )
    }

    fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        rpc_success(id, tool_schema())
    }

    async fn handle_tools_call(&self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => return rpc_error(id, -32602, "Missing params".into()),
        };

        let name = match params.get("name").and_then(Value::as_str) {
            Some(n) => n.to_owned(),
            None => return rpc_error(id, -32602, "Missing tool name".into()),
        };

        let arguments = params.get("arguments");
        let tenant = extract_tenant(arguments);

        match name.as_str() {
            "hydra.search_entities" => self.call_search_entities(id, tenant, arguments).await,
            "hydra.get_entity" => self.call_get_entity(id, tenant, arguments).await,
            "hydra.propose_envelope" => self.call_propose_envelope(id, tenant, arguments).await,
            "hydra.list_pending" => self.call_list_pending(id, tenant).await,
            "hydra.approve" => self.call_approve(id, tenant, arguments).await,
            "hydra.pipeline_stats" => self.call_pipeline_stats(id, tenant, arguments).await,
            "hydra.tk_stats" => self.call_tk_stats(id, arguments).await,
            other => rpc_error(id, -32601, format!("Unknown tool: {other}")),
        }
    }

    // -- tool implementations --

    async fn call_search_entities(
        &self,
        id: Option<Value>,
        tenant: Uuid,
        arguments: Option<&Value>,
    ) -> JsonRpcResponse {
        let kind = arguments
            .and_then(|a| a.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("");
        match self.entities.list(tenant, kind, None, 200).await {
            Ok(entities) => rpc_success(id, mcp_text_result(&entities)),
            Err(e) => rpc_error(id, -32603, format!("search_entities failed: {e}")),
        }
    }

    async fn call_get_entity(
        &self,
        id: Option<Value>,
        tenant: Uuid,
        arguments: Option<&Value>,
    ) -> JsonRpcResponse {
        let kind = arguments
            .and_then(|a| a.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let entity_id = arguments
            .and_then(|a| a.get("id"))
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok());
        let entity_id = match entity_id {
            Some(eid) => eid,
            None => return rpc_error(id, -32602, "Missing or invalid entity id".into()),
        };
        match self.entities.get(tenant, kind, entity_id).await {
            Ok(entity) => rpc_success(id, mcp_text_result(&entity)),
            Err(e) => rpc_error(id, -32603, format!("get_entity failed: {e}")),
        }
    }

    async fn call_propose_envelope(
        &self,
        id: Option<Value>,
        tenant: Uuid,
        arguments: Option<&Value>,
    ) -> JsonRpcResponse {
        let args = match arguments {
            Some(a) => a,
            None => return rpc_error(id, -32602, "Missing tool arguments".into()),
        };

        let domain = match args.get("domain").and_then(Value::as_str) {
            Some(d) => d.to_owned(),
            None => return rpc_error(id, -32602, "Missing domain".into()),
        };
        let action = match args.get("action").and_then(Value::as_str) {
            Some(a) => a.to_owned(),
            None => return rpc_error(id, -32602, "Missing action".into()),
        };
        let targets: Vec<Uuid> = args
            .get("targets")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                    .collect()
            })
            .unwrap_or_default();
        let payload = args.get("payload").cloned().unwrap_or(Value::Null);
        let rationale = match args.get("rationale").and_then(Value::as_str) {
            Some(r) => r.to_owned(),
            None => return rpc_error(id, -32602, "Missing rationale".into()),
        };
        let kind = args.get("kind").and_then(Value::as_str).map(String::from);

        let request = EnvelopeCreateRequest {
            domain,
            action,
            kind,
            targets,
            payload,
            rationale,
            reversal: Reversal::Compensating,
            blast: BlastRadiusDto {
                entities: 0,
                external_sends: 0,
                money_cents: 0,
                pii_egress: false,
            },
        };

        match self.envelopes.propose(tenant, request).await {
            Ok(envelope) => rpc_success(id, mcp_text_result(&envelope)),
            Err(e) => rpc_error(id, -32603, format!("propose_envelope failed: {e}")),
        }
    }

    async fn call_list_pending(
        &self,
        id: Option<Value>,
        tenant: Uuid,
    ) -> JsonRpcResponse {
        match self.envelopes.list(tenant, EnvelopeState::PendingApproval).await {
            Ok(envelopes) => rpc_success(id, mcp_text_result(&envelopes)),
            Err(e) => rpc_error(id, -32603, format!("list_pending failed: {e}")),
        }
    }

    async fn call_approve(
        &self,
        id: Option<Value>,
        tenant: Uuid,
        arguments: Option<&Value>,
    ) -> JsonRpcResponse {
        let envelope_id = arguments
            .and_then(|a| a.get("id"))
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok());
        let envelope_id = match envelope_id {
            Some(eid) => eid,
            None => return rpc_error(id, -32602, "Missing or invalid envelope id".into()),
        };
        match self.envelopes.approve(tenant, envelope_id).await {
            Ok(envelope) => rpc_success(id, mcp_text_result(&envelope)),
            Err(e) => rpc_error(id, -32603, format!("approve failed: {e}")),
        }
    }

    async fn call_pipeline_stats(
        &self,
        id: Option<Value>,
        tenant: Uuid,
        _arguments: Option<&Value>,
    ) -> JsonRpcResponse {
        let pending = match self.envelopes.list(tenant, EnvelopeState::PendingApproval).await {
            Ok(e) => e.len(),
            Err(_) => 0,
        };
        let approved = match self.envelopes.list(tenant, EnvelopeState::Approved).await {
            Ok(e) => e.len(),
            Err(_) => 0,
        };
        let executed = match self.envelopes.list(tenant, EnvelopeState::Executed).await {
            Ok(e) => e.len(),
            Err(_) => 0,
        };

        let stats = json!({
            "pending": pending,
            "approved": approved,
            "executed": executed,
        });
        rpc_success(id, mcp_text_result(&stats))
    }

    async fn call_tk_stats(
        &self,
        id: Option<Value>,
        arguments: Option<&Value>,
    ) -> JsonRpcResponse {
        let window = arguments
            .and_then(|a| a.get("window"))
            .and_then(Value::as_str)
            .unwrap_or("24h");
        match self.tk_stats.window(window).await {
            Ok(stats) => rpc_success(id, mcp_text_result(&stats)),
            Err(e) => rpc_error(id, -32603, format!("tk_stats failed: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Axum HTTP handler
// ---------------------------------------------------------------------------

pub async fn mcp_route(
    State(state): State<FabricAppState>,
    body: Json<Value>,
) -> Json<JsonRpcResponse> {
    let request: JsonRpcRequest = match serde_json::from_value(body.0) {
        Ok(req) => req,
        Err(e) => {
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id: None,
                result: None,
                error: Some(JsonRpcErrorBody {
                    code: -32700,
                    message: format!("Parse error: {e}"),
                    data: None,
                }),
            });
        }
    };
    let handler = McpHandler::new(&state);
    let response = handler.handle_request(request).await;
    Json(response)
}

// ---------------------------------------------------------------------------
// Tool schema
// ---------------------------------------------------------------------------

pub fn tool_schema() -> Value {
    json!({
        "protocol": "mcp",
        "version": "1.0.0",
        "tools": [
            {
                "name": "hydra.search_entities",
                "description": "Search for entities by kind with an optional query filter",
                "inputSchema": {
                    "type": "object",
                    "required": ["kind", "query"],
                    "properties": {
                        "kind": { "type": "string" },
                        "query": { "type": "string" }
                    }
                }
            },
            {
                "name": "hydra.get_entity",
                "description": "Retrieve a single entity by kind and id",
                "inputSchema": {
                    "type": "object",
                    "required": ["kind", "id"],
                    "properties": {
                        "kind": { "type": "string" },
                        "id": { "type": "string", "format": "uuid" }
                    }
                }
            },
            {
                "name": "hydra.propose_envelope",
                "description": "Propose a new governance envelope for approval",
                "inputSchema": {
                    "type": "object",
                    "required": ["domain", "action", "targets", "payload", "rationale"],
                    "properties": {
                        "domain": { "type": "string" },
                        "action": { "type": "string" },
                        "targets": {
                            "type": "array",
                            "items": { "type": "string", "format": "uuid" }
                        },
                        "payload": { "type": "object" },
                        "rationale": { "type": "string" }
                    }
                }
            },
            {
                "name": "hydra.list_pending",
                "description": "List all envelopes awaiting approval",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "hydra.approve",
                "description": "Approve a pending envelope by id",
                "inputSchema": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {
                        "id": { "type": "string", "format": "uuid" }
                    }
                }
            },
            {
                "name": "hydra.pipeline_stats",
                "description": "Return high-level envelope pipeline statistics",
                "inputSchema": {
                    "type": "object",
                    "required": ["pipeline_id"],
                    "properties": {
                        "pipeline_id": { "type": "string", "format": "uuid" }
                    }
                }
            },
            {
                "name": "hydra.tk_stats",
                "description": "Return token-killer hit-ratio stats for a time window",
                "inputSchema": {
                    "type": "object",
                    "required": ["window"],
                    "properties": {
                        "window": { "type": "string" }
                    }
                }
            }
        ]
    })
}
