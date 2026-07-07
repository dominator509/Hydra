use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;

use crate::error::FabricError;
use crate::services::{
    dev_admin_actor_from_headers, tenant_from_headers, AppState, AutonomyCellDto,
};

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
    headers: HeaderMap,
    Json(cells): Json<Vec<AutonomyCellDto>>,
) -> Result<Json<Vec<AutonomyCellDto>>, FabricError> {
    let tenant = tenant_from_headers(&headers)?;
    let actor = dev_admin_actor_from_headers(&headers)?;
    let cells = state.autonomy.replace(tenant, actor, cells).await?;
    Ok(Json(cells))
}
