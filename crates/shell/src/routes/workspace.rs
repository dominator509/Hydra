use askama::Template;
use axum::extract::Extension;
use axum::http::StatusCode;
use axum::response::IntoResponse;

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

pub async fn workspace_home(Extension(ctx): Extension<fabric::AuthCtx>) -> impl IntoResponse {
    let token = CsrfToken::generate();
    let page = routes::PageCtx::new("Workspace", "workspace", Some(ctx.tenant), &token);
    let template = WorkspaceTemplate {
        title: page.title,
        tenant: page.tenant,
        current_page: page.current_page,
        flash: page.flash,
        csrf: page.csrf,
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
