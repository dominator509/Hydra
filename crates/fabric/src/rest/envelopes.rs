use axum::extract::{Path, Query, State};
use axum::Json;
use governor::EnvelopeState;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::FabricError;
use crate::services::{tenant_from_headers, AppState, EnvelopeCreateRequest};

#[derive(Debug, Deserialize)]
pub struct EnvelopeListQuery {
    pub state: String,
}

pub async fn list_envelopes(
    State(app_state): State<AppState>,
    Query(query): Query<EnvelopeListQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<governor::ActionEnvelope>>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let envelope_state = parse_state(&query.state)?;
    let envelopes = app_state.envelopes.list(tenant, envelope_state).await?;
    Ok(Json(envelopes))
}

pub async fn propose_envelope(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<EnvelopeCreateRequest>,
) -> Result<Json<governor::ActionEnvelope>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let envelope = state.envelopes.propose(tenant, request).await?;
    Ok(Json(envelope))
}

pub async fn approve_envelope(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<governor::ActionEnvelope>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let envelope = state.envelopes.approve(tenant, id).await?;
    Ok(Json(envelope))
}

pub async fn reject_envelope(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<governor::ActionEnvelope>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let envelope = state.envelopes.reject(tenant, id).await?;
    Ok(Json(envelope))
}

fn parse_state(raw: &str) -> Result<EnvelopeState, FabricError> {
    match raw {
        "Proposed" => Ok(EnvelopeState::Proposed),
        "PendingApproval" => Ok(EnvelopeState::PendingApproval),
        "Approved" => Ok(EnvelopeState::Approved),
        "Executing" => Ok(EnvelopeState::Executing),
        "Executed" => Ok(EnvelopeState::Executed),
        "Failed" => Ok(EnvelopeState::Failed),
        "RolledBack" => Ok(EnvelopeState::RolledBack),
        "Rejected" => Ok(EnvelopeState::Rejected),
        other => Err(FabricError::ValidationFailed(format!(
            "unknown envelope state '{other}'"
        ))),
    }
}
