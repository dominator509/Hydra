use axum::extract::{Extension, Path, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::auth::AuthCtx;
use crate::error::FabricError;
use crate::services::{tenant_from_headers, AppState, BridgeRegisterRequest, BridgeStatusDto};

pub async fn register_bridge(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    headers: HeaderMap,
    Json(request): Json<BridgeRegisterRequest>,
) -> Result<Json<governor::ActionEnvelope>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let envelope = state
        .bridges
        .register(&ctx, tenant, &ctx.principal, request)
        .await?;
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
    Extension(ctx): Extension<AuthCtx>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<BridgeStatusDto>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let status = state
        .bridges
        .pause(&ctx, tenant, &ctx.principal, &id)
        .await?;
    Ok(Json(status))
}

pub async fn resume_bridge(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<BridgeStatusDto>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let status = state
        .bridges
        .resume(&ctx, tenant, &ctx.principal, &id)
        .await?;
    Ok(Json(status))
}
