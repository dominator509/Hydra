use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::jwt::{TokenClaims, TokenScope, TokenService};
use crate::error::FabricError;
use crate::services::AppState;

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct TokenRequest {
    grant_type: String,
    client_id: Option<String>,
    client_secret: Option<String>,
    code: Option<String>,
    scope: Option<String>,
}

#[derive(Serialize)]
pub struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: i64,
    scope: String,
}

pub async fn token_endpoint(
    State(_state): State<AppState>,
    Json(request): Json<TokenRequest>,
) -> Result<impl IntoResponse, FabricError> {
    let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001")
        .expect("dev tenant");

    let scopes = if let Some(ref s) = request.scope {
        TokenScope::parse_all(s)
    } else {
        vec![TokenScope::ReadCdm]
    };

    let subject = if let Some(ref id) = request.client_id {
        format!("service:{}", id)
    } else {
        "anonymous".into()
    };

    let claims = TokenClaims::new(subject, tenant, &scopes, 1);
    let token_service = TokenService::new(b"dev-secret-key-hydra-ep-006-m4-token!".to_vec());
    let access_token = token_service
        .sign(&claims)
        .map_err(|e| FabricError::Internal(e.to_string()))?;

    let expires_in = claims.exp - claims.iat;

    Ok((
        StatusCode::OK,
        Json(TokenResponse {
            access_token,
            token_type: "bearer".into(),
            expires_in,
            scope: claims.scope.clone(),
        }),
    ))
}
