use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::error::FabricError;
use crate::services::{
    dev_admin_actor_from_headers, tenant_from_headers, AppState, BridgeRegisterRequest,
    BridgeStatusDto,
};

pub async fn register_bridge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<BridgeRegisterRequest>,
) -> Result<Json<governor::ActionEnvelope>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let actor = dev_admin_actor_from_headers(&headers)?;
    let envelope = state.bridges.register(tenant, actor, request).await?;
    Ok(Json(envelope))
}

pub async fn bridge_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<BridgeStatusDto>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let status = state.bridges.status(tenant, &id).await?;
    Ok(Json(status))
}

pub async fn pause_bridge(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<BridgeStatusDto>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let actor = dev_admin_actor_from_headers(&headers)?;
    let status = state.bridges.pause(tenant, actor, &id).await?;
    Ok(Json(status))
}

pub async fn resume_bridge(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<BridgeStatusDto>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let actor = dev_admin_actor_from_headers(&headers)?;
    let status = state.bridges.resume(tenant, actor, &id).await?;
    Ok(Json(status))
}
