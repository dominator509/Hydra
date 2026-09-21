use axum::extract::{Extension, State};
use axum::http::{header::HeaderName, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::auth::{PrincipalContext, Scope};
use crate::error::FabricError;
use crate::services::AppState;

const A2A_VERSION: &str = "1.0";
const A2A_VERSION_HEADER: HeaderName = HeaderName::from_static("a2a-version");
const MAX_MESSAGE_ID_BYTES: usize = 200;
const MAX_CONTEXT_ID_BYTES: usize = 512;
const MAX_PARTS: usize = 32;
const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_HISTORY: usize = 50;

const WORKFLOW_MIGRATION_ASSESSMENT: &str = "migration-assessment";
const WORKFLOW_BRIDGE_SYNTHESIS: &str = "bridge-synthesis";
const WORKFLOW_BRIDGE_CONFORMANCE: &str = "bridge-conformance";
const KNOWN_WORKFLOWS: [&str; 6] = [
    WORKFLOW_MIGRATION_ASSESSMENT,
    "legacy-crm-discovery",
    "bridge-synthesis",
    "bridge-conformance",
    "bridge-wiring",
    "bridge-canary",
];

#[derive(Debug, Clone)]
pub struct JsonRpcRequest {
    id: Value,
    method: String,
    params: Value,
}

impl<'de> serde::Deserialize<'de> for JsonRpcRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("JSON-RPC request must be an object"))?;
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err(serde::de::Error::custom("jsonrpc must be '2.0'"));
        }
        let id = object
            .get("id")
            .cloned()
            .ok_or_else(|| serde::de::Error::custom("JSON-RPC id is required"))?;
        if id.is_null() {
            return Err(serde::de::Error::custom("JSON-RPC id cannot be null"));
        }
        let method = object
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| serde::de::Error::custom("JSON-RPC method is required"))?
            .to_owned();
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        Ok(Self { id, method, params })
    }
}

#[derive(Debug, serde::Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, serde::Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

pub async fn agent_card(State(state): State<AppState>) -> Json<Value> {
    let skills = KNOWN_WORKFLOWS
        .iter()
        .map(|workflow| {
            let (available, description, reason) = if *workflow == WORKFLOW_MIGRATION_ASSESSMENT {
                (
                    true,
                    "Deterministic, read-only migration assessment metadata workflow.",
                    None,
                )
            } else if *workflow == WORKFLOW_BRIDGE_SYNTHESIS {
                (
                    state.bridge_synthesis.available(),
                    "Proposal-only bounded CRM bridge mapping synthesis.",
                    Some(state.bridge_synthesis.availability_reason()),
                )
            } else if *workflow == WORKFLOW_BRIDGE_CONFORMANCE {
                (
                    state.bridge_conformance.available(),
                    "Read-only bounded conformance of a configured Hydra bridge adapter.",
                    Some(state.bridge_conformance.availability_reason()),
                )
            } else {
                (
                    false,
                    "Workflow is reserved and currently unavailable until a typed Hydra handler exists.",
                    Some("typed runtime handler is not registered"),
                )
            };
            json!({
                "id": format!("hydra.workflow.{workflow}"),
                "name": workflow,
                "description": description,
                "tags": ["hydra", "workflow"],
                "examples": [],
                "metadata": {
                    "available": available,
                    "reason": reason.map(Value::from).unwrap_or(Value::Null)
                }
            })
        })
        .collect::<Vec<_>>();
    Json(json!({
        "name": "Hydra CRM Workflow Boundary",
        "description": "Hydra-owned, authenticated long-running CRM workflow facade.",
        "url": "/a2a",
        "version": "1.0.0",
        "protocolVersion": A2A_VERSION,
        "capabilities": {
            "streaming": false,
            "pushNotifications": false,
            "stateTransitionHistory": true
        },
        "defaultInputModes": ["application/json"],
        "defaultOutputModes": ["application/json"],
        "skills": skills,
        "metadata": {
            "hydraIntegrationEnabled": state.nexus_control_plane.enabled,
            "authority": "Hydra",
            "crmMutationPath": "ActionEnvelope -> Governor -> typed handler"
        }
    }))
}

pub async fn json_rpc(
    State(state): State<AppState>,
    Extension(principal): Extension<PrincipalContext>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Response {
    if headers
        .get(&A2A_VERSION_HEADER)
        .and_then(|value| value.to_str().ok())
        != Some(A2A_VERSION)
    {
        return protocol_response(
            StatusCode::BAD_REQUEST,
            error_response(
                request.id,
                -32003,
                "A2A-Version must be exactly '1.0'",
                None,
            ),
        );
    }

    let response = dispatch(&state, &principal, request).await;
    Json(response).into_response()
}

async fn dispatch(
    state: &AppState,
    principal: &PrincipalContext,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.clone();
    if let Err(error) = state.authorization.authorize_external_scope(
        principal,
        Scope::BridgesRead,
        principal.hydra_tenant_id,
    ) {
        return protocol_error_from_fabric(id, error);
    }

    match request.method.as_str() {
        "SendMessage" => send_message(state, principal, id, request.params).await,
        "GetTask" => get_task(state, principal, id, request.params).await,
        "ListTasks" => list_tasks(state, principal, id, request.params).await,
        "CancelTask" => cancel_task(state, principal, id, request.params).await,
        _ => error_response(id, -32601, "method not found", None),
    }
}

async fn send_message(
    state: &AppState,
    principal: &PrincipalContext,
    id: Value,
    params: Value,
) -> JsonRpcResponse {
    let Some(params) = params.as_object() else {
        return error_response(id, -32602, "params must be an object", None);
    };
    let Some(message) = params.get("message").and_then(Value::as_object) else {
        return error_response(id, -32602, "SendMessage.message is required", None);
    };
    let Some(message_id) = bounded_string(message.get("messageId"), MAX_MESSAGE_ID_BYTES) else {
        return error_response(id, -32602, "messageId is invalid", None);
    };
    if message.get("role").and_then(Value::as_str) != Some("user") {
        return error_response(id, -32602, "message.role must be 'user'", None);
    }
    let Some(parts) = message.get("parts").and_then(Value::as_array) else {
        return error_response(id, -32602, "message.parts is required", None);
    };
    if parts.is_empty()
        || parts.len() > MAX_PARTS
        || serde_json::to_vec(parts).map_or(true, |v| v.len() > MAX_INPUT_BYTES)
    {
        return error_response(
            id,
            -32602,
            "message.parts exceeds the supported bound",
            None,
        );
    }
    let metadata = message
        .get("metadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let Some(workflow) = bounded_string(metadata.get("io.hydra/workflow"), MAX_MESSAGE_ID_BYTES)
    else {
        return error_response(id, -32602, "io.hydra/workflow is required", None);
    };
    if !KNOWN_WORKFLOWS.contains(&workflow.as_str()) {
        return error_response(id, -32602, "workflow is not recognized", None);
    }
    let workflow_available = match workflow.as_str() {
        WORKFLOW_MIGRATION_ASSESSMENT => true,
        WORKFLOW_BRIDGE_SYNTHESIS => state.bridge_synthesis.available(),
        WORKFLOW_BRIDGE_CONFORMANCE => state.bridge_conformance.available(),
        _ => false,
    };
    if !workflow_available {
        let reason = match workflow.as_str() {
            WORKFLOW_BRIDGE_SYNTHESIS => state.bridge_synthesis.availability_reason(),
            WORKFLOW_BRIDGE_CONFORMANCE => state.bridge_conformance.availability_reason(),
            _ => "typed runtime handler is not registered",
        };
        return error_response(
            id,
            -32004,
            "requested workflow is currently unavailable",
            Some(json!({"workflow": workflow, "reason": reason})),
        );
    }
    let input = metadata
        .get("io.hydra/input")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !input.is_object()
        || serde_json::to_vec(&input).map_or(true, |value| value.len() > MAX_INPUT_BYTES)
    {
        return error_response(
            id,
            -32602,
            "io.hydra/input must be a bounded JSON object",
            None,
        );
    }
    let context_id = message
        .get("contextId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("hydra:{}", principal.correlation.correlation_id));
    if context_id.trim().is_empty() || context_id.len() > MAX_CONTEXT_ID_BYTES {
        return error_response(id, -32602, "contextId is invalid", None);
    }
    let objective_id = optional_metadata_string(&metadata, "io.hydra/objectiveId");
    let causation_id = principal.correlation.causation_id.clone();
    let request_hash = request_hash(&workflow, &context_id, &input, objective_id.as_deref());
    let history = json!([history_event("submitted")]);
    let request = store::NewA2aTask {
        id: Uuid::new_v4(),
        tenant_id: principal.hydra_tenant_id,
        context_id,
        message_id,
        workflow,
        request_hash,
        requester_principal_id: principal.principal_id.clone(),
        requester_principal_type: principal_type_name(principal),
        correlation_id: principal.correlation.correlation_id.clone(),
        causation_id,
        objective_id,
        input,
        history,
    };
    let resolution = match state.a2a_tasks.create_or_get(request).await {
        Ok(resolution) => resolution,
        Err(error) => return task_error(id, error),
    };
    let task = match resolution {
        store::A2aTaskResolution::Created(task) | store::A2aTaskResolution::Existing(task) => {
            if task.workflow == WORKFLOW_MIGRATION_ASSESSMENT {
                resume_migration_task(state, principal.hydra_tenant_id, task).await
            } else if task.workflow == WORKFLOW_BRIDGE_SYNTHESIS {
                resume_bridge_synthesis_task(state, principal.hydra_tenant_id, task).await
            } else {
                resume_bridge_conformance_task(state, principal.hydra_tenant_id, task).await
            }
        }
    };
    match task {
        Ok(task) => success_response(id, task_value(&task, None)),
        Err(error) => task_error(id, error),
    }
}

async fn get_task(
    state: &AppState,
    principal: &PrincipalContext,
    id: Value,
    params: Value,
) -> JsonRpcResponse {
    let Some(task_id) = parse_task_id(&params) else {
        return error_response(id, -32602, "GetTask.id must be a UUID", None);
    };
    match state
        .a2a_tasks
        .get(principal.hydra_tenant_id, task_id)
        .await
    {
        Ok(task) => success_response(id, task_value(&task, history_length(&params))),
        Err(error) => task_error(id, error),
    }
}

async fn list_tasks(
    state: &AppState,
    principal: &PrincipalContext,
    id: Value,
    params: Value,
) -> JsonRpcResponse {
    let Some(params) = params.as_object() else {
        return error_response(id, -32602, "params must be an object", None);
    };
    if params
        .get("pageToken")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty())
    {
        return error_response(id, -32602, "pageToken is not supported", None);
    }
    let context_id = params
        .get("contextId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if context_id
        .as_deref()
        .is_some_and(|value| value.is_empty() || value.len() > MAX_CONTEXT_ID_BYTES)
    {
        return error_response(id, -32602, "contextId is invalid", None);
    }
    let limit = params
        .get("pageSize")
        .and_then(Value::as_u64)
        .unwrap_or(20)
        .clamp(1, 100) as i64;
    match state
        .a2a_tasks
        .list(principal.hydra_tenant_id, context_id.as_deref(), limit)
        .await
    {
        Ok(tasks) => success_response(
            id,
            json!({
                "tasks": tasks.iter().map(|task| task_value(task, None)).collect::<Vec<_>>(),
                "nextPageToken": ""
            }),
        ),
        Err(error) => task_error(id, error),
    }
}

async fn cancel_task(
    state: &AppState,
    principal: &PrincipalContext,
    id: Value,
    params: Value,
) -> JsonRpcResponse {
    let Some(task_id) = parse_task_id(&params) else {
        return error_response(id, -32602, "CancelTask.id must be a UUID", None);
    };
    let task = match state
        .a2a_tasks
        .get(principal.hydra_tenant_id, task_id)
        .await
    {
        Ok(task) => task,
        Err(error) => return task_error(id, error),
    };
    if !matches!(task.status.as_str(), "submitted" | "working") {
        return error_response(
            id,
            -32002,
            "task cannot be canceled in its current state",
            None,
        );
    }
    match state
        .a2a_tasks
        .transition(store::A2aTaskTransition {
            tenant_id: principal.hydra_tenant_id,
            id: task.id,
            expected_revision: task.revision,
            expected_status: task.status.clone(),
            new_status: "canceled".to_owned(),
            artifact: None,
            history_event: json!(history_event("canceled")),
        })
        .await
    {
        Ok(task) => success_response(id, task_value(&task, None)),
        Err(error) => task_error(id, error),
    }
}

async fn resume_migration_task(
    state: &AppState,
    tenant_id: Uuid,
    mut task: store::A2aTask,
) -> Result<store::A2aTask, FabricError> {
    for _ in 0..3 {
        match task.status.as_str() {
            "submitted" => {
                task = state
                    .a2a_tasks
                    .transition(store::A2aTaskTransition {
                        tenant_id,
                        id: task.id,
                        expected_revision: task.revision,
                        expected_status: "submitted".to_owned(),
                        new_status: "working".to_owned(),
                        artifact: None,
                        history_event: json!(history_event("working")),
                    })
                    .await?;
            }
            "working" => {
                let artifact = json!({
                    "workflow": WORKFLOW_MIGRATION_ASSESSMENT,
                    "available": true,
                    "mode": "deterministic_read_only",
                    "requestHash": task.request_hash,
                    "inputKeys": sorted_input_keys(&task.input)
                });
                return state
                    .a2a_tasks
                    .transition(store::A2aTaskTransition {
                        tenant_id,
                        id: task.id,
                        expected_revision: task.revision,
                        expected_status: "working".to_owned(),
                        new_status: "completed".to_owned(),
                        artifact: Some(artifact),
                        history_event: json!(history_event("completed")),
                    })
                    .await;
            }
            _ => return Ok(task),
        }
    }
    Ok(task)
}

async fn resume_bridge_synthesis_task(
    state: &AppState,
    tenant_id: Uuid,
    mut task: store::A2aTask,
) -> Result<store::A2aTask, FabricError> {
    for _ in 0..3 {
        match task.status.as_str() {
            "submitted" => {
                task = state
                    .a2a_tasks
                    .transition(store::A2aTaskTransition {
                        tenant_id,
                        id: task.id,
                        expected_revision: task.revision,
                        expected_status: "submitted".to_owned(),
                        new_status: "working".to_owned(),
                        artifact: None,
                        history_event: json!(history_event("working")),
                    })
                    .await?;
            }
            "working" => {
                let transition = match state
                    .bridge_synthesis
                    .synthesize(tenant_id, task.input.clone())
                    .await
                {
                    Ok(proposal) => store::A2aTaskTransition {
                        tenant_id,
                        id: task.id,
                        expected_revision: task.revision,
                        expected_status: "working".to_owned(),
                        new_status: "completed".to_owned(),
                        artifact: Some(json!({
                            "workflow": WORKFLOW_BRIDGE_SYNTHESIS,
                            "available": true,
                            "mode": "mapping_proposal",
                            "proposal": proposal
                        })),
                        history_event: json!(history_event("completed")),
                    },
                    Err(_) => store::A2aTaskTransition {
                        tenant_id,
                        id: task.id,
                        expected_revision: task.revision,
                        expected_status: "working".to_owned(),
                        new_status: "failed".to_owned(),
                        artifact: Some(json!({
                            "workflow": WORKFLOW_BRIDGE_SYNTHESIS,
                            "available": false,
                            "error": "synthesis_failed"
                        })),
                        history_event: json!(history_event("failed")),
                    },
                };
                return state.a2a_tasks.transition(transition).await;
            }
            _ => return Ok(task),
        }
    }
    Ok(task)
}

async fn resume_bridge_conformance_task(
    state: &AppState,
    tenant_id: Uuid,
    mut task: store::A2aTask,
) -> Result<store::A2aTask, FabricError> {
    for _ in 0..3 {
        match task.status.as_str() {
            "submitted" => {
                task = state
                    .a2a_tasks
                    .transition(store::A2aTaskTransition {
                        tenant_id,
                        id: task.id,
                        expected_revision: task.revision,
                        expected_status: "submitted".to_owned(),
                        new_status: "working".to_owned(),
                        artifact: None,
                        history_event: json!(history_event("working")),
                    })
                    .await?;
            }
            "working" => {
                let transition = match state
                    .bridge_conformance
                    .conform(tenant_id, task.input.clone())
                    .await
                {
                    Ok(result) => store::A2aTaskTransition {
                        tenant_id,
                        id: task.id,
                        expected_revision: task.revision,
                        expected_status: "working".to_owned(),
                        new_status: "completed".to_owned(),
                        artifact: Some(json!({
                            "workflow": WORKFLOW_BRIDGE_CONFORMANCE,
                            "available": true,
                            "mode": "metadata_only",
                            "conformance": result
                        })),
                        history_event: json!(history_event("completed")),
                    },
                    Err(_) => store::A2aTaskTransition {
                        tenant_id,
                        id: task.id,
                        expected_revision: task.revision,
                        expected_status: "working".to_owned(),
                        new_status: "failed".to_owned(),
                        artifact: Some(json!({
                            "workflow": WORKFLOW_BRIDGE_CONFORMANCE,
                            "available": false,
                            "error": "conformance_failed"
                        })),
                        history_event: json!(history_event("failed")),
                    },
                };
                return state.a2a_tasks.transition(transition).await;
            }
            _ => return Ok(task),
        }
    }
    Ok(task)
}

fn task_value(task: &store::A2aTask, requested_history_length: Option<usize>) -> Value {
    let history = task
        .history
        .as_array()
        .map(|history| {
            let length = requested_history_length
                .unwrap_or(MAX_HISTORY)
                .min(MAX_HISTORY);
            let start = history.len().saturating_sub(length);
            Value::Array(history[start..].to_vec())
        })
        .unwrap_or_else(|| json!([]));
    let artifacts = task
        .artifact
        .as_ref()
        .map(|artifact| {
            json!([{
                "artifactId": format!("{}:result", task.id),
                "parts": [{"kind": "data", "data": artifact}]
            }])
        })
        .unwrap_or_else(|| json!([]));
    json!({
        "id": task.id.to_string(),
        "contextId": task.context_id,
        "status": {
            "state": task.status,
            "timestamp": timestamp(task.updated_at)
        },
        "artifacts": artifacts,
        "history": history,
        "metadata": {
            "io.hydra/workflow": task.workflow,
            "io.hydra/correlationId": task.correlation_id,
            "io.hydra/objectiveId": task.objective_id
        }
    })
}

fn sorted_input_keys(input: &Value) -> Vec<String> {
    let Some(object) = input.as_object() else {
        return Vec::new();
    };
    let mut keys = object.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
}

fn parse_task_id(params: &Value) -> Option<Uuid> {
    params
        .get("id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn history_length(params: &Value) -> Option<usize> {
    params
        .get("historyLength")
        .and_then(Value::as_u64)
        .map(|value| (value as usize).min(MAX_HISTORY))
}

fn bounded_string(value: Option<&Value>, max_bytes: usize) -> Option<String> {
    let value = value?.as_str()?.trim();
    if value.is_empty()
        || value.len() > max_bytes
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:/".contains(&byte))
    {
        return None;
    }
    Some(value.to_owned())
}

fn optional_metadata_string(metadata: &Map<String, Value>, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(|value| bounded_string(Some(value), MAX_MESSAGE_ID_BYTES))
}

fn request_hash(
    workflow: &str,
    context_id: &str,
    input: &Value,
    objective_id: Option<&str>,
) -> String {
    let canonical = json!({
        "workflow": workflow,
        "contextId": context_id,
        "input": input,
        "objectiveId": objective_id
    });
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&canonical).unwrap_or_default());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn history_event(state: &str) -> Value {
    json!({"state": state, "at": timestamp(time::OffsetDateTime::now_utc())})
}

fn timestamp(value: time::OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn principal_type_name(principal: &PrincipalContext) -> String {
    match principal.principal_type {
        crate::auth::PrincipalType::Human => "human",
        crate::auth::PrincipalType::NexusService => "nexus_service",
        crate::auth::PrincipalType::NexusAgent => "nexus_agent",
        crate::auth::PrincipalType::HydraInternalAgent => "hydra_internal_agent",
        crate::auth::PrincipalType::LocalHydraUser => "local_hydra_user",
    }
    .to_owned()
}

fn success_response(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}

fn error_response(id: Value, code: i32, message: &str, data: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_owned(),
            data,
        }),
    }
}

fn protocol_error_from_fabric(id: Value, error: FabricError) -> JsonRpcResponse {
    match error {
        FabricError::AuthzDenied | FabricError::TenantMismatch => {
            error_response(id, -32003, "authorization denied", None)
        }
        FabricError::CapabilityUnavailable(_) => {
            error_response(id, -32004, "A2A capability unavailable", None)
        }
        _ => error_response(id, -32603, "internal error", None),
    }
}

fn task_error(id: Value, error: FabricError) -> JsonRpcResponse {
    match error {
        FabricError::NotFound | FabricError::TenantMismatch => {
            error_response(id, -32001, "task not found", None)
        }
        FabricError::IdempotencyConflict => error_response(
            id,
            -32005,
            "messageId conflicts with a different request",
            None,
        ),
        FabricError::VersionConflict => {
            error_response(id, -32002, "task changed concurrently", None)
        }
        FabricError::AuthzDenied => error_response(id, -32003, "authorization denied", None),
        FabricError::CapabilityUnavailable(_) => {
            error_response(id, -32004, "A2A capability unavailable", None)
        }
        _ => error_response(id, -32603, "internal error", None),
    }
}

fn protocol_response(status: StatusCode, body: JsonRpcResponse) -> Response {
    (status, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_hash_is_stable_and_does_not_include_secret_fields() {
        let hash = request_hash(
            "migration-assessment",
            "ctx",
            &json!({"adapter": "x"}),
            None,
        );
        assert_eq!(hash.len(), 64);
        assert!(!hash.contains("secret"));
        assert_eq!(
            hash,
            request_hash(
                "migration-assessment",
                "ctx",
                &json!({"adapter": "x"}),
                None
            )
        );
    }

    #[test]
    fn workflow_catalog_reports_only_one_available_handler() {
        assert!(KNOWN_WORKFLOWS.contains(&WORKFLOW_MIGRATION_ASSESSMENT));
        assert_eq!(
            KNOWN_WORKFLOWS
                .iter()
                .filter(|workflow| **workflow == WORKFLOW_MIGRATION_ASSESSMENT)
                .count(),
            1
        );
    }
}
