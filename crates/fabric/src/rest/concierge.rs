use axum::extract::{Extension, State};
use axum::Json;
use serde::Deserialize;

use crate::auth::AuthCtx;
use crate::error::FabricError;
use crate::services::{AppState, ConciergePingResponse};

#[derive(Deserialize)]
pub struct PingRequest {
    question: String,
}

pub async fn concierge_ping(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Json(request): Json<PingRequest>,
) -> Result<Json<ConciergePingResponse>, FabricError> {
    let response = state.concierge.ping(ctx.tenant, &request.question).await?;
    Ok(Json(response))
}
