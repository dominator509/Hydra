use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;

use crate::error::FabricError;
use crate::services::{tenant_from_headers, AppState, ConciergePingResponse};

#[derive(Deserialize)]
pub struct PingRequest {
    question: String,
}

pub async fn concierge_ping(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PingRequest>,
) -> Result<Json<ConciergePingResponse>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let response = state.concierge.ping(tenant, &request.question).await?;
    Ok(Json(response))
}
