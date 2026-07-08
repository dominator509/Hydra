use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use governor::{EnvelopeState, Reversal};
use serde::Deserialize;
use uuid::Uuid;

use crate::csrf::CsrfToken;
use crate::flash::FlashMessage;
use crate::routes;

#[derive(Template)]
#[template(path = "approvals.html")]
struct ApprovalsTemplate {
    title: String,
    tenant: String,
    current_page: String,
    flash: Vec<FlashMessage>,
    csrf: String,
    envelopes: Vec<EnvelopeRow>,
}

#[derive(Template)]
#[template(path = "approval_row.html")]
struct ApprovalRowTemplate {
    envelope: EnvelopeRow,
    csrf: String,
}

struct EnvelopeRow {
    id: String,
    domain: String,
    action: String,
    kind: Option<String>,
    rationale: String,
    state: String,
    state_css: String,
    reversal: Reversal,
    blast: BlastRow,
    targets: Vec<String>,
    history: Vec<TransitionRow>,
}

struct BlastRow {
    entities: u32,
    external_sends: u32,
    money_cents: u64,
    pii_egress: bool,
}

struct TransitionRow {
    from: String,
    to: String,
    actor: String,
    at_rfc3339: String,
}

#[derive(Deserialize)]
pub struct ApproveForm {
    _csrf_token: String,
}

#[derive(Deserialize)]
pub struct BatchActionForm {
    _csrf_token: String,
}

// ── Handlers ──

pub async fn approvals_list(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = CsrfToken::generate();
    let tenant = routes::tenant_or_default(&headers);

    let envelopes = match state.envelopes.list(tenant, EnvelopeState::PendingApproval).await {
        Ok(list) => list.into_iter().map(to_envelope_row).collect(),
        Err(e) => {
            let mut ctx = routes::PageCtx::new("Approvals", "approvals", &headers, &token);
            ctx = ctx.with_flash(FlashMessage::error(format!("Failed to load approvals: {e}")));
            let template = ApprovalsTemplate {
                title: ctx.title, tenant: ctx.tenant, current_page: ctx.current_page,
                flash: ctx.flash, csrf: ctx.csrf, envelopes: Vec::new(),
            };
            return template.render().map(|html| {
                (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
            }).unwrap_or_else(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            });
        }
    };

    let ctx = routes::PageCtx::new("Approvals", "approvals", &headers, &token);
    let template = ApprovalsTemplate {
        title: ctx.title, tenant: ctx.tenant, current_page: ctx.current_page,
        flash: ctx.flash, csrf: ctx.csrf, envelopes,
    };
    template.render().map(|html| {
        (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
    }).unwrap_or_else(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    })
}

pub async fn approve_envelope(
    State(state): State<fabric::AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Form(form): Form<ApproveForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }
    let tenant = routes::tenant_or_default(&headers);

    match state.envelopes.approve(tenant, id).await {
        Ok(envelope) => {
            let row = to_envelope_row(envelope);
            let t = ApprovalRowTemplate { envelope: row, csrf: CsrfToken::generate().hidden_field() };
            t.render().map(|html| {
                (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
            }).unwrap_or_else(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            })
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

pub async fn reject_envelope(
    State(state): State<fabric::AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Form(form): Form<ApproveForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }
    let tenant = routes::tenant_or_default(&headers);

    match state.envelopes.reject(tenant, id).await {
        Ok(envelope) => {
            let row = to_envelope_row(envelope);
            let t = ApprovalRowTemplate { envelope: row, csrf: CsrfToken::generate().hidden_field() };
            t.render().map(|html| {
                (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
            }).unwrap_or_else(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            })
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

pub async fn batch_approve(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
    Form(form): Form<BatchActionForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }
    let tenant = routes::tenant_or_default(&headers);

    let envelopes = match state.envelopes.list(tenant, EnvelopeState::PendingApproval).await {
        Ok(list) => list,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    };

    let mut approved = 0usize;
    for envelope in &envelopes {
        if state.envelopes.approve(tenant, envelope.id).await.is_ok() {
            approved += 1;
        }
    }

    let token = CsrfToken::generate();
    let ctx = routes::PageCtx::new("Approvals", "approvals", &headers, &token);
    let flash = vec![FlashMessage::success(format!("{approved} envelope(s) approved"))];

    let remaining = match state.envelopes.list(tenant, EnvelopeState::PendingApproval).await {
        Ok(list) => list.into_iter().map(to_envelope_row).collect(),
        Err(_) => Vec::new(),
    };

    let template = ApprovalsTemplate {
        title: ctx.title, tenant: ctx.tenant, current_page: ctx.current_page,
        flash, csrf: ctx.csrf, envelopes: remaining,
    };
    template.render().map(|html| {
        (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
    }).unwrap_or_else(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    })
}

pub async fn batch_reject(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
    Form(form): Form<BatchActionForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }
    let tenant = routes::tenant_or_default(&headers);

    let envelopes = match state.envelopes.list(tenant, EnvelopeState::PendingApproval).await {
        Ok(list) => list,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    };

    let mut rejected = 0usize;
    for envelope in &envelopes {
        if state.envelopes.reject(tenant, envelope.id).await.is_ok() {
            rejected += 1;
        }
    }

    let token = CsrfToken::generate();
    let ctx = routes::PageCtx::new("Approvals", "approvals", &headers, &token);
    let flash = vec![FlashMessage::success(format!("{rejected} envelope(s) rejected"))];

    let remaining = match state.envelopes.list(tenant, EnvelopeState::PendingApproval).await {
        Ok(list) => list.into_iter().map(to_envelope_row).collect(),
        Err(_) => Vec::new(),
    };

    let template = ApprovalsTemplate {
        title: ctx.title, tenant: ctx.tenant, current_page: ctx.current_page,
        flash, csrf: ctx.csrf, envelopes: remaining,
    };
    template.render().map(|html| {
        (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
    }).unwrap_or_else(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    })
}

pub async fn approvals_count(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let tenant = routes::tenant_or_default(&headers);
    let count = match state.envelopes.list(tenant, EnvelopeState::PendingApproval).await {
        Ok(list) => list.len(),
        Err(_) => 0,
    };
    (StatusCode::OK, count.to_string()).into_response()
}

// ── Helpers ──

fn to_envelope_row(e: governor::ActionEnvelope) -> EnvelopeRow {
    let state_str = envelope_state_label(e.state);
    EnvelopeRow {
        id: e.id.to_string(),
        domain: e.domain,
        action: e.action,
        kind: e.kind,
        rationale: e.rationale,
        state: state_str.to_owned(),
        state_css: state_str.to_lowercase(),
        reversal: e.reversal,
        blast: BlastRow {
            entities: e.blast.entities,
            external_sends: e.blast.external_sends,
            money_cents: e.blast.money_cents,
            pii_egress: e.blast.pii_egress,
        },
        targets: e.targets.iter().map(|t| t.to_string()).collect(),
        history: e.history.into_iter().map(|t| TransitionRow {
            from: format!("{:?}", t.from),
            to: format!("{:?}", t.to),
            actor: t.actor,
            at_rfc3339: t.at_rfc3339,
        }).collect(),
    }
}

fn envelope_state_label(state: EnvelopeState) -> &'static str {
    match state {
        EnvelopeState::Proposed => "Proposed",
        EnvelopeState::PendingApproval => "PendingApproval",
        EnvelopeState::Approved => "Approved",
        EnvelopeState::Executing => "Executing",
        EnvelopeState::Executed => "Executed",
        EnvelopeState::Failed => "Failed",
        EnvelopeState::RolledBack => "RolledBack",
        EnvelopeState::Rejected => "Rejected",
    }
}
