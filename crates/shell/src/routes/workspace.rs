use askama::Template;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect};

use crate::csrf::CsrfToken;
use crate::flash::FlashMessage;
use crate::routes;

#[derive(Template)]
#[template(path = "workspace.html")]
struct WorkspaceTemplate {
    title: String,
    tenant: String,
    current_page: String,
    flash: Vec<FlashMessage>,
    csrf: String,
    stats: WorkspaceStats,
}

struct WorkspaceStats {
    active_pipelines: usize,
    pending_approvals: usize,
    active_bridges: usize,
    agents_online: usize,
}

pub async fn workspace_home(headers: HeaderMap) -> impl IntoResponse {
    if routes::session_tenant(&headers).is_none() {
        return Redirect::to("/login").into_response();
    }

    let token = CsrfToken::generate();
    let ctx = routes::PageCtx::new("Workspace", "workspace", &headers, &token);
    let template = WorkspaceTemplate {
        title: ctx.title,
        tenant: ctx.tenant,
        current_page: ctx.current_page,
        flash: ctx.flash,
        csrf: ctx.csrf,
        stats: WorkspaceStats {
            active_pipelines: 0,
            pending_approvals: 0,
            active_bridges: 0,
            agents_online: 0,
        },
    };
    match template.render() {
        Ok(html) => (
            StatusCode::OK,
            [("content-type", "text/html; charset=utf-8")],
            html,
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
