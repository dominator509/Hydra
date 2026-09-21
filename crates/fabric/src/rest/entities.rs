use axum::extract::{Extension, Path, Query, State};
use axum::http::{header::IF_MATCH, HeaderMap};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::auth::{AuthCtx, Role};
use crate::error::FabricError;
use crate::services::{AppState, EntityDeleteResponse};

#[derive(Debug, Deserialize)]
pub struct EntityListQuery {
    pub cursor: Option<Uuid>,
    pub limit: Option<u16>,
}

pub async fn list_entities(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path(kind): Path<String>,
    Query(query): Query<EntityListQuery>,
) -> Result<Json<Vec<cdm::Entity>>, FabricError> {
    let limit = parse_limit(query.limit)?;
    let entities = state
        .entities
        .list(ctx.tenant, &kind, query.cursor, limit)
        .await?;
    Ok(Json(entities))
}

pub async fn create_entity(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path(kind): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<cdm::Entity>, FabricError> {
    ctx.require_role(Role::Operator)?;
    let entity = state.entities.create(ctx.tenant, &kind, body).await?;
    Ok(Json(entity))
}

pub async fn get_entity(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<Json<cdm::Entity>, FabricError> {
    let entity = state.entities.get(ctx.tenant, &kind, id).await?;
    Ok(Json(entity))
}

pub async fn patch_entity(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path((kind, id)): Path<(String, Uuid)>,
    headers: HeaderMap,
    Json(patch): Json<Value>,
) -> Result<Json<cdm::Entity>, FabricError> {
    ctx.require_role(Role::Operator)?;
    let version = if_match_version(&headers)?;
    let entity = state
        .entities
        .patch(ctx.tenant, &kind, id, version, patch)
        .await?;
    Ok(Json(entity))
}

pub async fn delete_entity(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<Json<EntityDeleteResponse>, FabricError> {
    ctx.require_role(Role::Operator)?;
    let response = state.entities.delete(ctx.tenant, &kind, id).await?;
    Ok(Json(response))
}

fn parse_limit(raw: Option<u16>) -> Result<u16, FabricError> {
    match raw {
        None => Ok(50),
        Some(limit) if (1..=200).contains(&limit) => Ok(limit),
        Some(limit) => Err(FabricError::ValidationFailed(format!(
            "limit must be between 1 and 200, got {limit}"
        ))),
    }
}

fn if_match_version(headers: &HeaderMap) -> Result<u64, FabricError> {
    let raw = headers
        .get(IF_MATCH)
        .ok_or_else(|| FabricError::ValidationFailed("If-Match header is required".into()))?
        .to_str()
        .map_err(|_| FabricError::ValidationFailed("If-Match must be utf-8".into()))?;
    let raw = raw.trim();
    let raw = raw.strip_prefix("W/").unwrap_or(raw);
    let raw = raw.trim_matches('"');
    raw.parse::<u64>().map_err(|error| {
        FabricError::ValidationFailed(format!("invalid If-Match version: {error}"))
    })
}
