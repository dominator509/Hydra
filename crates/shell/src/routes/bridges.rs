use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::csrf::CsrfToken;
use crate::flash::FlashMessage;
use crate::routes;

#[derive(Template)]
#[template(path = "bridges.html")]
struct BridgesTemplate {
    title: String,
    tenant: String,
    current_page: String,
    flash: Vec<FlashMessage>,
    csrf: String,
    bridges: Vec<BridgeView>,
}

struct BridgeView {
    adapter_id: String,
    state: String,
    envelope_id: Option<String>,
    envelope_state: Option<String>,
    wiring_ref: Option<String>,
}

#[derive(Deserialize)]
pub struct RegisterBridgeForm {
    _csrf_token: String,
    adapter_id: String,
    wiring_ref: String,
    rationale: String,
    fuel: u64,
    origins: Option<String>,
    secret_names: Option<String>,
}

#[derive(Deserialize)]
pub struct BridgeActionForm {
    _csrf_token: String,
}

// ── Handlers ──

pub async fn bridges_list(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = CsrfToken::generate();
    let tenant = routes::tenant_or_default(&headers);

    let adapter_ids = ["memcrm", "hubspot", "slack", "outlook", "teams"];
    let mut views = Vec::new();
    for aid in &adapter_ids {
        if let Ok(status) = state.bridges.status(tenant, aid).await {
            views.push(BridgeView {
                adapter_id: status.adapter_id,
                state: status.state,
                envelope_id: status.envelope_id.map(|id| id.to_string()),
                envelope_state: status.envelope_state,
                wiring_ref: status.wiring_ref,
            });
        }
    }

    let ctx = routes::PageCtx::new("Bridges", "bridges", &headers, &token);
    let template = BridgesTemplate {
        title: ctx.title,
        tenant: ctx.tenant,
        current_page: ctx.current_page,
        flash: ctx.flash,
        csrf: ctx.csrf,
        bridges: views,
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

pub async fn register_bridge(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
    Form(form): Form<RegisterBridgeForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }
    let tenant = routes::tenant_or_default(&headers);
    let ctx = routes::auth_ctx_from_headers(&headers);

    let adapter_id = form.adapter_id.clone();
    let request = fabric::BridgeRegisterRequest {
        adapter_id: form.adapter_id,
        wiring_ref: form.wiring_ref,
        rationale: form.rationale,
        grant: fabric::BridgeGrantDto {
            origins: form
                .origins
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect(),
            secret_names: form
                .secret_names
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect(),
            dsn_name: None,
            fuel: form.fuel,
        },
    };

    let actor = "dev-admin";

    match state.bridges.register(&ctx, tenant, actor, request).await {
        Ok(_) => {
            if let Ok(status) = state.bridges.status(tenant, &adapter_id).await {
                let csrf = CsrfToken::generate().as_str().to_owned();
                let wiring_html = status
                    .wiring_ref
                    .as_deref()
                    .map(|w| format!("&#128279; {w}"))
                    .unwrap_or_default();
                let button = if status.state == "active" {
                    format!("<form action=\"/bridges/{0}/pause\" method=\"post\" hx-post=\"/bridges/{0}/pause\" hx-target=\"#bridge-{0}\" hx-swap=\"outerHTML\"><input type=\"hidden\" name=\"_csrf_token\" value=\"{1}\"><button type=\"submit\">Pause</button></form>", adapter_id, csrf)
                } else {
                    String::new()
                };
                let html = format!("<div class=\"bridge-card\" id=\"bridge-{0}\"><div class=\"bridge-info\"><div class=\"bridge-adapter\">{0}</div><div class=\"bridge-meta\"><span class=\"chip chip-active\">{1}</span>{2}</div></div><div class=\"bridge-actions\">{3}</div></div>", adapter_id, status.state, wiring_html, button);
                return (
                    StatusCode::OK,
                    [("content-type", "text/html; charset=utf-8")],
                    html,
                )
                    .into_response();
            }
            (StatusCode::OK, "Bridge registered").into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

pub async fn pause_bridge(
    State(state): State<fabric::AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<BridgeActionForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }
    let tenant = routes::tenant_or_default(&headers);
    let ctx = routes::auth_ctx_from_headers(&headers);
    let actor = "dev-admin";

    match state.bridges.pause(&ctx, tenant, actor, &id).await {
        Ok(s) => {
            let csrf = CsrfToken::generate().as_str().to_owned();
            let html = format!(
                "<div class=\"bridge-card\" id=\"bridge-{id}\">\
                 <div class=\"bridge-info\"><div class=\"bridge-adapter\">{id}</div>\
                 <div class=\"bridge-meta\"><span class=\"chip chip-inactive\">{state}</span></div></div>\
                 <div class=\"bridge-actions\">\
                 <form action=\"/bridges/{id}/resume\" method=\"post\" \
                 hx-post=\"/bridges/{id}/resume\" hx-target=\"#bridge-{id}\" hx-swap=\"outerHTML\">\
                 <input type=\"hidden\" name=\"_csrf_token\" value=\"{csrf}\">\
                 <button type=\"submit\">Resume</button></form></div></div>",
                id = id, csrf = csrf, state = s.state
            );
            (
                StatusCode::OK,
                [("content-type", "text/html; charset=utf-8")],
                html,
            )
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

pub async fn resume_bridge(
    State(state): State<fabric::AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<BridgeActionForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }
    let tenant = routes::tenant_or_default(&headers);
    let ctx = routes::auth_ctx_from_headers(&headers);
    let actor = "dev-admin";

    match state.bridges.resume(&ctx, tenant, actor, &id).await {
        Ok(s) => {
            let csrf = CsrfToken::generate().as_str().to_owned();
            let html = format!(
                "<div class=\"bridge-card\" id=\"bridge-{id}\">\
                 <div class=\"bridge-info\"><div class=\"bridge-adapter\">{id}</div>\
                 <div class=\"bridge-meta\"><span class=\"chip chip-active\">{state}</span></div></div>\
                 <div class=\"bridge-actions\">\
                 <form action=\"/bridges/{id}/pause\" method=\"post\" \
                 hx-post=\"/bridges/{id}/pause\" hx-target=\"#bridge-{id}\" hx-swap=\"outerHTML\">\
                 <input type=\"hidden\" name=\"_csrf_token\" value=\"{csrf}\">\
                 <button type=\"submit\">Pause</button></form></div></div>",
                id = id, csrf = csrf, state = s.state
            );
            (
                StatusCode::OK,
                [("content-type", "text/html; charset=utf-8")],
                html,
            )
                .into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}
