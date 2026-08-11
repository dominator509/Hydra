use axum::http::StatusCode;

use crate::error::FabricError;

pub async fn token_endpoint() -> Result<StatusCode, FabricError> {
    Err(FabricError::CapabilityUnavailable(
        "Hydra is an OAuth/OIDC resource server and does not issue access tokens; obtain a short-lived token from the configured authorization server"
            .to_owned(),
    ))
}
