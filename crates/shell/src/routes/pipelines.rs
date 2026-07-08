use std::collections::BTreeMap;

use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use governor::Reversal;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::csrf::CsrfToken;
use crate::flash::FlashMessage;
use crate::routes;

// ── Pipeline Board Template ──

#[derive(Template)]
#[template(path = "pipeline_board.html")]
struct PipelineBoardTemplate {
    title: String,
    tenant: String,
    current_page: String,
    flash: Vec<FlashMessage>,
    csrf: String,
    stages: Vec<StageGroup>,
}

struct StageGroup {
    name: String,
    stage_key: String,
    deals: Vec<DealCard>,
}

struct DealCard {
    id: String,
    title: String,
    formatted_value: String,
    agent_created: bool,
}

// ── Record View Template ──

#[derive(Template)]
#[template(path = "record_view.html")]
struct RecordViewTemplate {
    title: String,
    tenant: String,
    current_page: String,
    flash: Vec<FlashMessage>,
    csrf: String,
    entity: RecordEntityView,
    events: Vec<EventView>,
}

struct RecordEntityView {
    id: String,
    kind: String,
    origin: String,
    origin_ref: Option<String>,
    version: u64,
    agent_state: Option<AgentStateView>,
    fields: Vec<FieldView>,
}

struct AgentStateView {
    css_class: String,
    label: String,
}

struct FieldView {
    name: String,
    value: String,
    url: Option<String>,
    code: bool,
}

struct EventView {
    actor: String,
    kind: String,
    summary: String,
    timestamp: String,
}

// ── Action Button Template ──

#[derive(Template)]
#[template(path = "action_button.html")]
struct ActionButtonTemplate {
    result_state: Option<ActionResult>,
    action_url: String,
    action_name: String,
    confirm_text: String,
    entity_id: String,
    button_label: String,
    csrf: String,
}

struct ActionResult {
    css_class: String,
    label: String,
    message: String,
}

// ── Forms ──

#[derive(Deserialize)]
pub struct ActionForm {
    _csrf_token: String,
    action: String,
}

#[derive(Deserialize)]
pub struct MoveForm {
    _csrf_token: String,
    to_stage: String,
}

#[derive(Deserialize)]
pub struct NewDealForm {
    _csrf_token: String,
    title: String,
    stage: String,
    value_cents: Option<u64>,
}

// ── Handlers ──

#[allow(clippy::let_and_return)]
pub async fn pipeline_board(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = CsrfToken::generate();
    let tenant = routes::tenant_or_default(&headers);

    let entities = match state.entities.list(tenant, "deal", None, 200).await {
        Ok(list) => list,
        Err(e) => {
            let mut ctx = routes::PageCtx::new("Pipeline Board", "pipelines", &headers, &token);
            ctx = ctx.with_flash(FlashMessage::error(format!("Failed to load deals: {e}")));
            let template = PipelineBoardTemplate {
                title: ctx.title,
                tenant: ctx.tenant,
                current_page: ctx.current_page,
                flash: ctx.flash,
                csrf: ctx.csrf,
                stages: Vec::new(),
            };
            return template.render().map(|html| {
                (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html)
                    .into_response()
            }).unwrap_or_else(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            });
        }
    };

    let mut grouped: BTreeMap<String, Vec<DealCard>> = BTreeMap::new();
    for entity in &entities {
        let stage = extract_stage(&entity.body).to_owned();
        let entry = grouped.entry(stage.clone()).or_default();
        entry.push(DealCard {
            id: entity.id.to_string(),
            title: extract_title(&entity.body).to_owned(),
            formatted_value: format_value(&entity.body),
            agent_created: entity.origin == "agent",
        });
    }

    let stages: Vec<StageGroup> = grouped
        .into_iter()
        .map(|(name, deals)| StageGroup {
            stage_key: name.to_lowercase().replace(' ', "_"),
            name,
            deals,
        })
        .collect();

    let ctx = routes::PageCtx::new("Pipeline Board", "pipelines", &headers, &token);
    let template = PipelineBoardTemplate {
        title: ctx.title,
        tenant: ctx.tenant,
        current_page: ctx.current_page,
        flash: ctx.flash,
        csrf: ctx.csrf,
        stages,
    };
    template.render().map(|html| {
        (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
    }).unwrap_or_else(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    })
}

pub async fn pipeline_record(
    State(state): State<fabric::AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = CsrfToken::generate();
    let tenant = routes::tenant_or_default(&headers);

    let entity = match state.entities.get(tenant, "deal", id).await {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                [("content-type", "text/plain; charset=utf-8")],
                format!("Deal not found: {e}"),
            )
                .into_response()
        }
    };

    let fields = build_fields(&entity.body);
    let events: Vec<EventView> = Vec::new();

    let ctx = routes::PageCtx::new(
        &format!("{} - {}", entity.kind, entity.id),
        "pipelines",
        &headers,
        &token,
    );

    let template = RecordViewTemplate {
        title: ctx.title,
        tenant: ctx.tenant,
        current_page: ctx.current_page,
        flash: ctx.flash,
        csrf: ctx.csrf,
        entity: RecordEntityView {
            id: entity.id.to_string(),
            kind: entity.kind,
            origin: entity.origin,
            origin_ref: entity.origin_ref,
            version: entity.version,
            agent_state: None,
            fields,
        },
        events,
    };
    template.render().map(|html| {
        (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
    }).unwrap_or_else(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    })
}

pub async fn pipeline_action(
    State(state): State<fabric::AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Form(form): Form<ActionForm>,
) -> impl IntoResponse {
    let token = CsrfToken::generate();
    let tenant = routes::tenant_or_default(&headers);

    let (result_state, csrf) = if let Err(flash) = routes::verify_csrf(&headers, &form._csrf_token) {
        (
            Some(ActionResult {
                css_class: "chip-failed".into(),
                label: "CSRF Error".into(),
                message: flash.text,
            }),
            token.hidden_field(),
        )
    } else {
        let create_request = fabric::EnvelopeCreateRequest {
            domain: "pipeline".into(),
            action: form.action.clone(),
            kind: Some("deal".into()),
            targets: vec![id],
            payload: json!({ "entity_id": id }),
            rationale: format!("Agent action '{}' on deal {id}", form.action),
            reversal: Reversal::Compensating,
            blast: fabric::BlastRadiusDto {
                entities: 1,
                external_sends: 0,
                money_cents: 0,
                pii_egress: false,
            },
        };

        match state.envelopes.propose(tenant, create_request).await {
            Ok(envelope) => {
                let label = envelope_state_label(envelope.state);
                let css_class = chip_class_for_state(envelope.state);
                (
                    Some(ActionResult {
                        css_class: css_class.into(),
                        label: label.to_owned(),
                        message: format!("Action proposed as {label}"),
                    }),
                    token.hidden_field(),
                )
            }
            Err(e) => (
                Some(ActionResult {
                    css_class: "chip-failed".into(),
                    label: "Error".into(),
                    message: format!("{e}"),
                }),
                token.hidden_field(),
            ),
        }
    };

    let template = ActionButtonTemplate {
        result_state,
        action_url: format!("/pipelines/{id}/action"),
        action_name: form.action.clone(),
        confirm_text: "Execute this action?".into(),
        entity_id: id.to_string(),
        button_label: "Retry".into(),
        csrf,
    };
    template.render().map(|html| {
        (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
    }).unwrap_or_else(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    })
}

pub async fn pipeline_move(
    State(state): State<fabric::AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Form(form): Form<MoveForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }

    let tenant = routes::tenant_or_default(&headers);

    let entity = match state.entities.get(tenant, "deal", id).await {
        Ok(e) => e,
        Err(_) => return (StatusCode::NOT_FOUND, "Deal not found").into_response(),
    };

    let mut patch = json!({ "stage": form.to_stage });
    if let Some(body) = entity.body.as_object() {
        for (k, v) in body {
            if k != "stage" {
                patch[k] = v.clone();
            }
        }
    }

    match state.entities.patch(tenant, "deal", id, entity.version, patch).await {
        Ok(_) => (
            StatusCode::OK,
            [("content-type", "text/html; charset=utf-8")],
            format!(
                r#"<div class="deal-card" id="deal-{id}"><div class="deal-title">Moved to {}</div></div>"#,
                form.to_stage
            ),
        ).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

pub async fn pipeline_new_deal(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
    Form(form): Form<NewDealForm>,
) -> impl IntoResponse {
    if routes::verify_csrf(&headers, &form._csrf_token).is_err() {
        return (StatusCode::FORBIDDEN, "CSRF mismatch").into_response();
    }

    let tenant = routes::tenant_or_default(&headers);

    let body = json!({
        "title": form.title,
        "stage": form.stage,
        "value_cents": form.value_cents.unwrap_or(0),
    });

    match state.entities.create(tenant, "deal", body).await {
        Ok(_) => (StatusCode::FOUND, [("location", "/pipelines")], ()).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    }
}

// ── Helpers ──

fn extract_title(body: &serde_json::Value) -> &str {
    body.get("title").and_then(serde_json::Value::as_str).unwrap_or("untitled")
}

fn extract_stage(body: &serde_json::Value) -> &str {
    body.get("stage").and_then(serde_json::Value::as_str).unwrap_or("unknown")
}

fn format_value(body: &serde_json::Value) -> String {
    let cents = body.get("value_cents").and_then(serde_json::Value::as_u64).unwrap_or(0);
    if cents >= 100_000_000 {
        format!("${:.1}M", cents as f64 / 100_000_000.0)
    } else if cents >= 100_000 {
        format!("${:.0}K", cents as f64 / 100_000.0)
    } else {
        format!("${:.2}", cents as f64 / 100.0)
    }
}

fn build_fields(body: &serde_json::Value) -> Vec<FieldView> {
    let obj = match body.as_object() {
        Some(o) => o,
        None => return Vec::new(),
    };
    obj.iter().map(|(key, val)| FieldView {
        name: key.clone(),
        value: match val {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => val.to_string(),
        },
        url: None,
        code: false,
    }).collect()
}

fn envelope_state_label(state: governor::EnvelopeState) -> &'static str {
    match state {
        governor::EnvelopeState::Proposed => "Suggested",
        governor::EnvelopeState::PendingApproval => "PendingApproval",
        governor::EnvelopeState::Approved => "Approved",
        governor::EnvelopeState::Executing => "Executing",
        governor::EnvelopeState::Executed => "Executed",
        governor::EnvelopeState::Failed => "Failed",
        governor::EnvelopeState::RolledBack => "RolledBack",
        governor::EnvelopeState::Rejected => "Rejected",
    }
}

fn chip_class_for_state(state: governor::EnvelopeState) -> &'static str {
    match state {
        governor::EnvelopeState::Proposed => "chip-queued",
        governor::EnvelopeState::PendingApproval => "chip-pendingapproval",
        governor::EnvelopeState::Approved => "chip-approved",
        governor::EnvelopeState::Executing => "chip-executing",
        governor::EnvelopeState::Executed => "chip-executed",
        governor::EnvelopeState::Failed => "chip-failed",
        governor::EnvelopeState::RolledBack => "chip-rolledback",
        governor::EnvelopeState::Rejected => "chip-rejected",
    }
}
