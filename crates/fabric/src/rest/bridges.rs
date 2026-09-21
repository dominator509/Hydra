use axum::extract::{Extension, Path, State};
use axum::Json;

use crate::auth::AuthCtx;
use crate::error::FabricError;
use crate::services::{AppState, BridgeRegisterRequest, BridgeStatusDto};

pub async fn register_bridge(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Json(request): Json<BridgeRegisterRequest>,
) -> Result<Json<governor::ActionEnvelope>, FabricError> {
    let envelope = state
        .bridges
        .register(&ctx, ctx.tenant, &ctx.principal, request)
        .await?;
    Ok(Json(envelope))
}

pub async fn bridge_status(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path(id): Path<String>,
) -> Result<Json<BridgeStatusDto>, FabricError> {
    let status = state.bridges.status(ctx.tenant, &id).await?;
    Ok(Json(status))
}

pub async fn pause_bridge(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path(id): Path<String>,
) -> Result<Json<BridgeStatusDto>, FabricError> {
    let status = state
        .bridges
        .pause(&ctx, ctx.tenant, &ctx.principal, &id)
        .await?;
    Ok(Json(status))
}

pub async fn resume_bridge(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path(id): Path<String>,
) -> Result<Json<BridgeStatusDto>, FabricError> {
    let status = state
        .bridges
        .resume(&ctx, ctx.tenant, &ctx.principal, &id)
        .await?;
    Ok(Json(status))
}
