use axum::extract::{Extension, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::auth::{AuthCtx, Role};
use crate::error::FabricError;
use crate::services::AppState;

#[derive(Debug, Deserialize)]
pub struct TenantExportQuery {
    pub max_records: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub struct RetentionPreviewQuery {
    pub age_days: Option<u16>,
}

pub async fn export(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Query(query): Query<TenantExportQuery>,
) -> Result<Json<store::TenantDataExport>, FabricError> {
    ctx.require_role(Role::Admin)?;
    let max_records = query.max_records.unwrap_or(10_000);
    validate_max_records(max_records)?;
    Ok(Json(
        state
            .tenant_data
            .export(ctx.tenant, i64::from(max_records))
            .await?,
    ))
}

pub async fn retention_preview(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthCtx>,
    Query(query): Query<RetentionPreviewQuery>,
) -> Result<Json<store::RetentionPreview>, FabricError> {
    ctx.require_role(Role::Admin)?;
    let age_days = query.age_days.unwrap_or(30);
    validate_age_days(age_days)?;
    Ok(Json(
        state
            .tenant_data
            .retention_preview(ctx.tenant, age_days)
            .await?,
    ))
}

fn validate_max_records(max_records: u16) -> Result<(), FabricError> {
    if (1..=store::MAX_EXPORT_RECORDS as u16).contains(&max_records) {
        Ok(())
    } else {
        Err(FabricError::ValidationFailed(format!(
            "max_records must be between 1 and {}",
            store::MAX_EXPORT_RECORDS
        )))
    }
}

fn validate_age_days(age_days: u16) -> Result<(), FabricError> {
    if (1..=3650).contains(&age_days) {
        Ok(())
    } else {
        Err(FabricError::ValidationFailed(
            "age_days must be between 1 and 3650".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_are_fail_closed() {
        assert!(validate_max_records(1).is_ok());
        assert!(validate_max_records(store::MAX_EXPORT_RECORDS as u16).is_ok());
        assert!(validate_max_records(0).is_err());
        assert!(validate_age_days(1).is_ok());
        assert!(validate_age_days(3650).is_ok());
        assert!(validate_age_days(0).is_err());
        assert!(validate_age_days(3651).is_err());
    }
}
