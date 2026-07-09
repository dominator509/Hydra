use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProblemJson {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub code: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl ProblemJson {
    pub fn new(code: &str, title: &str, detail: Option<String>) -> Self {
        Self {
            problem_type: format!("https://hydra.local/problems/{code}"),
            code: code.to_owned(),
            title: title.to_owned(),
            detail,
            instance: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FabricError {
    #[error("validation_failed: {0}")]
    ValidationFailed(String),
    #[error("authn_failed: {0}")]
    #[allow(dead_code)]
    AuthnFailed(String),
    #[error("not_found")]
    NotFound,
    #[error("tenant_mismatch")]
    TenantMismatch,
    #[error("version_conflict")]
    VersionConflict,
    #[error("authz_denied")]
    AuthzDenied,
    #[error("rate_limited")]
    RateLimited,
    #[error("llm_provider_error: {0}")]
    LlmProviderError(String),
    #[error("tk_output_nuked: {0}")]
    TkOutputNuked(String),
    #[error("tk_pii_route_blocked: {0}")]
    TkPiiRouteBlocked(String),
    #[error("constitution_blocked: {0}")]
    ConstitutionBlocked(String),
    #[error("cell_manual_only")]
    CellManualOnly,
    #[error("internal: {0}")]
    Internal(String),
}

impl FabricError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ValidationFailed(_) => "validation_failed",
            Self::AuthnFailed(_) => "authn_failed",
            Self::NotFound => "not_found",
            Self::TenantMismatch => "tenant_mismatch",
            Self::VersionConflict => "version_conflict",
            Self::AuthzDenied => "authz_denied",
            Self::RateLimited => "rate_limited",
            Self::LlmProviderError(_) => "llm_provider_error",
            Self::TkOutputNuked(_) => "tk_output_nuked",
            Self::TkPiiRouteBlocked(_) => "tk_pii_route_blocked",
            Self::ConstitutionBlocked(_) => "constitution_blocked",
            Self::CellManualOnly => "cell_manual_only",
            Self::Internal(_) => "internal",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::ValidationFailed(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::AuthnFailed(_) => StatusCode::UNAUTHORIZED,
            Self::NotFound | Self::TenantMismatch => StatusCode::NOT_FOUND,
            Self::VersionConflict => StatusCode::CONFLICT,
            Self::AuthzDenied | Self::ConstitutionBlocked(_) | Self::CellManualOnly => {
                StatusCode::FORBIDDEN
            }
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::LlmProviderError(_) | Self::TkOutputNuked(_) => StatusCode::BAD_GATEWAY,
            Self::TkPiiRouteBlocked(_) => StatusCode::BAD_REQUEST,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::ValidationFailed(_) => "Validation failed",
            Self::AuthnFailed(_) => "Authentication failed",
            Self::NotFound => "Not found",
            Self::TenantMismatch => "Tenant mismatch",
            Self::VersionConflict => "Version conflict",
            Self::AuthzDenied => "Authorization denied",
            Self::RateLimited => "Rate limited",
            Self::LlmProviderError(_) => "LLM provider error",
            Self::TkOutputNuked(_) => "TOKENKILLER output nuked",
            Self::TkPiiRouteBlocked(_) => "PII route blocked",
            Self::ConstitutionBlocked(_) => "Constitution blocked",
            Self::CellManualOnly => "Autonomy cell is manual only",
            Self::Internal(_) => "Internal error",
        }
    }

    pub fn detail(&self) -> Option<String> {
        match self {
            Self::ValidationFailed(detail)
            | Self::AuthnFailed(detail)
            | Self::LlmProviderError(detail)
            | Self::TkOutputNuked(detail)
            | Self::TkPiiRouteBlocked(detail)
            | Self::ConstitutionBlocked(detail) => Some(detail.clone()),
            Self::Internal(_) => None,
            _ => None,
        }
    }
}

impl IntoResponse for FabricError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ProblemJson::new(self.code(), self.title(), self.detail());
        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for FabricError {
    fn from(value: sqlx::Error) -> Self {
        Self::Internal(value.to_string())
    }
}

impl From<store::StoreError> for FabricError {
    fn from(value: store::StoreError) -> Self {
        match value {
            store::StoreError::Conflict(_) => Self::VersionConflict,
            store::StoreError::NotFound => Self::NotFound,
            store::StoreError::TenantMismatch => Self::TenantMismatch,
            store::StoreError::SchemaViolation { path, message } => {
                Self::ValidationFailed(format!("{path}: {message}"))
            }
            store::StoreError::UnknownKind(kind) => {
                Self::ValidationFailed(format!("unknown kind '{kind}'"))
            }
            other => Self::Internal(other.to_string()),
        }
    }
}
