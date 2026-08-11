use axum::extract::{Extension, State};
use axum::http::HeaderMap;
use axum::Json;

use crate::auth::AuthCtx;
use crate::error::FabricError;
use crate::services::{tenant_from_headers, AppState, AutonomyCellDto};

pub async fn list_cells(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AutonomyCellDto>>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let cells = state.autonomy.list(tenant).await?;
    Ok(Json(cells))
}

pub async fn replace_cells(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    headers: HeaderMap,
    Json(cells): Json<Vec<AutonomyCellDto>>,
) -> Result<Json<Vec<AutonomyCellDto>>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let cells = state
        .autonomy
        .replace(&ctx, tenant, &ctx.principal, cells)
        .await?;
    Ok(Json(cells))
}
