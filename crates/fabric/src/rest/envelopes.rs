use axum::extract::{Extension, Path, Query, State};
use axum::Json;
use governor::EnvelopeState;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::{AuthCtx, Role};
use crate::error::FabricError;
use crate::services::{AppState, EnvelopeCreateRequest};

#[derive(Debug, Deserialize)]
pub struct EnvelopeListQuery {
    pub state: String,
}

pub async fn list_envelopes(
    State(app_state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Query(query): Query<EnvelopeListQuery>,
) -> Result<Json<Vec<governor::ActionEnvelope>>, FabricError> {
    let envelope_state = parse_state(&query.state)?;
    let envelopes = app_state.envelopes.list(ctx.tenant, envelope_state).await?;
    Ok(Json(envelopes))
}

pub async fn propose_envelope(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Json(request): Json<EnvelopeCreateRequest>,
) -> Result<Json<governor::ActionEnvelope>, FabricError> {
    ctx.require_role(Role::Operator)?;
    let envelope = state.envelopes.propose(ctx.tenant, request).await?;
    Ok(Json(envelope))
}

pub async fn approve_envelope(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<governor::ActionEnvelope>, FabricError> {
    let envelope = state.envelopes.approve(&ctx, ctx.tenant, id).await?;
    Ok(Json(envelope))
}

pub async fn reject_envelope(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<governor::ActionEnvelope>, FabricError> {
    let envelope = state.envelopes.reject(&ctx, ctx.tenant, id).await?;
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
