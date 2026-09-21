use axum::extract::{Extension, State};
use axum::Json;

use crate::auth::AuthCtx;
use crate::error::FabricError;
use crate::services::{AppState, AutonomyCellDto};

pub async fn list_cells(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
) -> Result<Json<Vec<AutonomyCellDto>>, FabricError> {
    let cells = state.autonomy.list(ctx.tenant).await?;
    Ok(Json(cells))
}

pub async fn replace_cells(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Json(cells): Json<Vec<AutonomyCellDto>>,
) -> Result<Json<Vec<AutonomyCellDto>>, FabricError> {
    let cells = state
        .autonomy
        .replace(&ctx, ctx.tenant, &ctx.principal, cells)
        .await?;
    Ok(Json(cells))
}
