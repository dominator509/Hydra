use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::error::FabricError;
use crate::services::{AppState, TkWindowStats};

#[derive(Debug, Deserialize)]
pub struct TkWindowQuery {
    pub window: String,
}

pub async fn ledger_window(
    State(state): State<AppState>,
    Query(query): Query<TkWindowQuery>,
) -> Result<Json<TkWindowStats>, FabricError> {
    let stats = state.tk_stats.window(&query.window).await?;
    Ok(Json(stats))
}
