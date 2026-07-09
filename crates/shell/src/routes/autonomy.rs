use std::collections::BTreeSet;

use askama::Template;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::csrf::CsrfToken;
use crate::flash::FlashMessage;
use crate::routes;

#[derive(Template)]
#[template(path = "autonomy.html")]
struct AutonomyTemplate {
    title: String,
    tenant: String,
    current_page: String,
    flash: Vec<FlashMessage>,
    csrf: String,
    actions: Vec<String>,
    matrix_rows: Vec<MatrixRow>,
}

struct MatrixRow {
    domain: String,
    columns: Vec<MatrixCol>,
}

struct MatrixCol {
    action: String,
    level: String,
    level_lower: String,
    has_cell: bool,
    kind: Option<String>,
}

#[derive(Deserialize)]
pub struct SaveMatrixForm {
    _csrf_token: String,
}

#[derive(Deserialize)]
pub struct SaveKindsForm {
    _csrf_token: String,
}

// ── Handlers ──

pub async fn autonomy_page(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = CsrfToken::generate();
    let tenant = routes::tenant_or_default(&headers);

    let cells = match state.autonomy.list(tenant).await {
        Ok(list) => list,
        Err(e) => {
            let mut ctx = routes::PageCtx::new("Autonomy", "autonomy", &headers, &token);
            ctx = ctx.with_flash(FlashMessage::error(format!("Failed to load autonomy: {e}")));
            let template = AutonomyTemplate {
                title: ctx.title,
                tenant: ctx.tenant,
                current_page: ctx.current_page,
                flash: ctx.flash,
                csrf: ctx.csrf,
                actions: Vec::new(),
                matrix_rows: Vec::new(),
            };
            return template
                .render()
                .map(|html| {
                    (
                        StatusCode::OK,
                        [("content-type", "text/html; charset=utf-8")],
                        html,
                    )
                        .into_response()
                })
                .unwrap_or_else(|e| {
                    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
                });
        }
    };

    let mut action_set = BTreeSet::new();
    for cell in &cells {
        action_set.insert(cell.action.clone());
    }
    let actions: Vec<String> = action_set.into_iter().collect();

    let mut domain_map = std::collections::BTreeMap::new();
    for cell in cells {
        domain_map
            .entry(cell.domain.clone())
            .or_insert_with(Vec::new)
            .push(cell);
    }

    let mut matrix_rows = Vec::new();
    for (domain, domain_cells) in &domain_map {
        let mut columns = Vec::new();
        for action in &actions {
            if let Some(cell) = domain_cells.iter().find(|c| &c.action == action) {
                columns.push(MatrixCol {
                    action: action.clone(),
                    level: cell.level.clone(),
                    level_lower: cell.level.to_lowercase(),
                    has_cell: true,
                    kind: cell.kind.clone(),
                });
            } else {
                columns.push(MatrixCol {
                    action: action.clone(),
                    level: "L0".into(),
                    level_lower: "l0".into(),
                    has_cell: false,
                    kind: None,
                });
            }
        }
        matrix_rows.push(MatrixRow {
            domain: domain.clone(),
            columns,
        });
    }

    let ctx = routes::PageCtx::new("Autonomy", "autonomy", &headers, &token);
    let template = AutonomyTemplate {
        title: ctx.title,
        tenant: ctx.tenant,
        current_page: ctx.current_page,
        flash: ctx.flash,
        csrf: ctx.csrf,
        actions,
        matrix_rows,
    };
    template
        .render()
        .map(|html| {
            (
                StatusCode::OK,
                [("content-type", "text/html; charset=utf-8")],
                html,
            )
                .into_response()
        })
        .unwrap_or_else(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response())
}

pub async fn save_matrix(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
    Form(form): Form<SaveMatrixForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }
    let tenant = routes::tenant_or_default(&headers);
    let ctx = routes::auth_ctx_from_headers(&headers);
    let actor = "dev-admin";

    let current = state.autonomy.list(tenant).await.unwrap_or_default();
    match state.autonomy.replace(&ctx, tenant, actor, current).await {
        Ok(_) => (StatusCode::OK, "Matrix saved").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

pub async fn save_kinds(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
    Form(form): Form<SaveKindsForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }
    let tenant = routes::tenant_or_default(&headers);
    let ctx = routes::auth_ctx_from_headers(&headers);
    let actor = "dev-admin";

    let current = state.autonomy.list(tenant).await.unwrap_or_default();
    match state.autonomy.replace(&ctx, tenant, actor, current).await {
        Ok(_) => (StatusCode::OK, "Kind overrides saved").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}
