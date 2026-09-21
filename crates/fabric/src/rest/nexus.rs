use axum::extract::{Extension, Path, Request, State};
use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::auth::{CorrelationContext, PrincipalContext, Scope};
use crate::error::FabricError;
use crate::mcp::execute_capability;
use crate::services::{AppState, EnvelopeApprovalReceipt, EnvelopeApprovalRequest};
use crate::trace_context::{insert_trace_headers, server_trace_context};

const REQUEST_ID_HEADER: &str = "x-request-id";
const CORRELATION_ID_HEADER: &str = "x-correlation-id";
const CAUSATION_ID_HEADER: &str = "x-causation-id";

pub async fn external_auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    if !state.nexus_control_plane.enabled {
        return FabricError::CapabilityUnavailable("Nexus interoperability is disabled".to_owned())
            .into_response();
    }
    let Some(authenticator) = state.external_auth.as_ref() else {
        return FabricError::CapabilityUnavailable(
            "Nexus identity validation is not configured".to_owned(),
        )
        .into_response();
    };

    let correlation = match correlation_from_headers(request.headers()) {
        Ok(correlation) => correlation,
        Err(error) => return error.into_response(),
    };
    let trace = server_trace_context(request.headers());
    let authorization = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let mut principal = match authenticator
        .authenticate(authorization, correlation.clone())
        .await
    {
        Ok(principal) => principal,
        Err(error) => return auth_error_response(&state, error),
    };
    principal.trace = trace.clone();
    let rate_key = format!(
        "principal:{}:{}",
        principal.hydra_tenant_id, principal.principal_id
    );
    if let Err(error) = state.rate_limiter.check_async(&rate_key).await {
        return error.into_response();
    }
    request.extensions_mut().insert(principal);

    let mut response = next.run(request).await;
    insert_response_header(
        response.headers_mut(),
        REQUEST_ID_HEADER,
        &correlation.request_id,
    );
    insert_response_header(
        response.headers_mut(),
        CORRELATION_ID_HEADER,
        &correlation.correlation_id,
    );
    insert_trace_headers(response.headers_mut(), &trace);
    response
}

pub async fn capabilities(
    State(state): State<AppState>,
    Extension(principal): Extension<PrincipalContext>,
) -> Result<Json<Value>, FabricError> {
    let value =
        execute_capability(&state, &principal, "hydra.capabilities.list", Map::new()).await?;
    Ok(Json(value))
}

pub async fn context(
    State(state): State<AppState>,
    Extension(principal): Extension<PrincipalContext>,
) -> Result<Json<Value>, FabricError> {
    let value = execute_capability(&state, &principal, "hydra.crm.context", Map::new()).await?;
    Ok(Json(value))
}

pub async fn propose_stage_change(
    State(state): State<AppState>,
    Extension(principal): Extension<PrincipalContext>,
    Json(arguments): Json<Value>,
) -> Result<Json<Value>, FabricError> {
    let arguments = arguments.as_object().cloned().ok_or_else(|| {
        FabricError::ValidationFailed("proposal body must be a JSON object".to_owned())
    })?;
    let value =
        execute_capability(&state, &principal, "hydra.crm.propose_action", arguments).await?;
    Ok(Json(value))
}

pub async fn propose_bridge_sync(
    State(state): State<AppState>,
    Extension(principal): Extension<PrincipalContext>,
    Path(adapter_id): Path<String>,
    Json(arguments): Json<Value>,
) -> Result<Json<Value>, FabricError> {
    let mut arguments = arguments.as_object().cloned().ok_or_else(|| {
        FabricError::ValidationFailed("proposal body must be a JSON object".to_owned())
    })?;
    if arguments.contains_key("adapter_id") {
        return Err(FabricError::ValidationFailed(
            "adapter_id is bound by the REST path".to_owned(),
        ));
    }
    arguments.insert("adapter_id".to_owned(), Value::String(adapter_id));
    let value = execute_capability(&state, &principal, "hydra.bridges.sync", arguments).await?;
    Ok(Json(value))
}

pub async fn decide_envelope_approval(
    State(state): State<AppState>,
    Extension(principal): Extension<PrincipalContext>,
    Path(id): Path<Uuid>,
    Json(request): Json<EnvelopeApprovalRequest>,
) -> Result<Json<EnvelopeApprovalReceipt>, FabricError> {
    let receipt = state
        .envelopes
        .decide_external_approval(&principal, id, request)
        .await?;
    Ok(Json(receipt))
}

pub async fn bindings(
    State(state): State<AppState>,
    Extension(principal): Extension<PrincipalContext>,
) -> Result<Json<Value>, FabricError> {
    state.authorization.authorize_external_scope(
        &principal,
        Scope::CapabilitiesRead,
        principal.hydra_tenant_id,
    )?;
    Ok(Json(json!({
        "bindings": [{
            "id": principal.binding_id,
            "provider": principal.external_provider,
            "external_tenant_id": principal.external_tenant_id,
            "external_business_id": principal.external_business_id,
            "hydra_tenant_id": principal.hydra_tenant_id,
            "status": "active"
        }]
    })))
}

pub async fn events_status(
    State(state): State<AppState>,
    Extension(principal): Extension<PrincipalContext>,
) -> Result<Json<Value>, FabricError> {
    state.authorization.authorize_external_scope(
        &principal,
        Scope::CapabilitiesRead,
        principal.hydra_tenant_id,
    )?;
    Ok(Json(
        serde_json::to_value(state.event_status.status().await?)
            .map_err(|error| FabricError::Internal(format!("serialize event status: {error}")))?,
    ))
}

#[derive(Debug, Serialize)]
pub struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    scopes_supported: Vec<&'static str>,
    bearer_methods_supported: Vec<&'static str>,
    resource_name: &'static str,
}

pub async fn protected_resource_metadata(
    State(state): State<AppState>,
) -> Result<Json<ProtectedResourceMetadata>, FabricError> {
    if !state.nexus_control_plane.enabled {
        return Err(FabricError::CapabilityUnavailable(
            "Nexus interoperability is disabled".to_owned(),
        ));
    }
    Ok(Json(ProtectedResourceMetadata {
        resource: state.nexus_control_plane.resource.clone(),
        authorization_servers: state.nexus_control_plane.authorization_servers.clone(),
        scopes_supported: all_scopes(),
        bearer_methods_supported: vec!["header"],
        resource_name: "Hydra CRM Control Plane",
    }))
}

fn correlation_from_headers(headers: &HeaderMap) -> Result<CorrelationContext, FabricError> {
    let request_id = optional_identifier(headers, REQUEST_ID_HEADER)?
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let correlation_id =
        optional_identifier(headers, CORRELATION_ID_HEADER)?.unwrap_or_else(|| request_id.clone());
    let causation_id = optional_identifier(headers, CAUSATION_ID_HEADER)?;
    Ok(CorrelationContext {
        request_id,
        correlation_id,
        causation_id,
    })
}

fn optional_identifier(
    headers: &HeaderMap,
    name: &'static str,
) -> Result<Option<String>, FabricError> {
    let Some(value) = headers.get(name) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| FabricError::ValidationFailed(format!("invalid {name} header")))?;
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:/".contains(&byte))
    {
        return Err(FabricError::ValidationFailed(format!(
            "invalid {name} header"
        )));
    }
    Ok(Some(value.to_owned()))
}

fn auth_error_response(state: &AppState, error: FabricError) -> Response {
    let should_challenge = matches!(error, FabricError::AuthnFailed(_));
    let mut response = error.into_response();
    if should_challenge {
        let challenge = format!(
            "Bearer resource_metadata=\"{}\", scope=\"hydra.capabilities.read\"",
            state.nexus_control_plane.resource_metadata_url
        );
        if let Ok(value) = HeaderValue::from_str(&challenge) {
            response.headers_mut().insert(WWW_AUTHENTICATE, value);
        }
    }
    response
}

fn insert_response_header(headers: &mut HeaderMap, name: &'static str, value: &str) {
    if let (Ok(name), Ok(value)) = (
        HeaderName::from_bytes(name.as_bytes()),
        HeaderValue::from_str(value),
    ) {
        headers.insert(name, value);
    }
}

fn all_scopes() -> Vec<&'static str> {
    [
        Scope::CapabilitiesRead,
        Scope::CrmRead,
        Scope::CrmContextRead,
        Scope::CrmPropose,
        Scope::EnvelopesRead,
        Scope::EnvelopesApprove,
        Scope::BridgesRead,
        Scope::BridgesAdmin,
        Scope::AutonomyRead,
        Scope::AutonomyAdmin,
    ]
    .into_iter()
    .map(Scope::as_str)
    .collect()
}
