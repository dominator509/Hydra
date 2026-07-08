use askama::Template;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};

use crate::csrf::CsrfToken;
use crate::flash::FlashMessage;
use crate::routes;

#[derive(Template)]
#[template(path = "agents.html")]
struct AgentsTemplate {
    title: String,
    tenant: String,
    current_page: String,
    flash: Vec<FlashMessage>,
    csrf: String,
    agents: Vec<AgentView>,
}

struct AgentView {
    name: String,
    online: bool,
    route_name: String,
    hit_ratio: Option<f64>,
    hit_pct: u32,
    recent_actions: Vec<ActionView>,
    tk_window_stats: Option<TkWindowStat>,
}

#[derive(Clone)]
struct ActionView {
    name: String,
    timestamp: String,
}

struct TkWindowStat {
    window: String,
    routes: Vec<RouteStat>,
}

struct RouteStat {
    route: String,
    hit_ratio: Option<f64>,
    hit_pct: u32,
}

// ── Handlers ──

pub async fn agents_console(
    State(state): State<fabric::AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = CsrfToken::generate();
    let tenant = routes::tenant_or_default(&headers);
    let _ = tenant;

    let window_stats = match state.tk_stats.window("24h").await {
        Ok(stats) => stats,
        Err(e) => {
            let mut ctx = routes::PageCtx::new("Agent Console", "agents", &headers, &token);
            ctx = ctx.with_flash(FlashMessage::error(format!("Failed to load TK stats: {e}")));
            let template = AgentsTemplate {
                title: ctx.title, tenant: ctx.tenant, current_page: ctx.current_page,
                flash: ctx.flash, csrf: ctx.csrf, agents: Vec::new(),
            };
            return template.render().map(|html| {
                (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
            }).unwrap_or_else(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            });
        }
    };

    let empty_actions: Vec<ActionView> = Vec::new();
    let mut agents: Vec<AgentView> = window_stats.routes.into_iter().map(|r| {
        let hit_pct = r.hit_ratio.map(|h| (h * 100.0) as u32).unwrap_or(0);
        AgentView {
            name: r.route.clone(),
            online: r.hit_ratio.map(|h| h > 0.0).unwrap_or(false),
            route_name: r.route.clone(),
            hit_ratio: r.hit_ratio,
            hit_pct,
            recent_actions: empty_actions.clone(),
            tk_window_stats: Some(TkWindowStat {
                window: "24h".into(),
                routes: vec![RouteStat { route: r.route.clone(), hit_ratio: r.hit_ratio, hit_pct: r.hit_ratio.map(|h| (h * 100.0) as u32).unwrap_or(0) }],
            }),
        }
    }).collect();

    if agents.is_empty() {
        agents.push(AgentView {
            name: "concierge".into(), online: false, route_name: "concierge".into(),
            hit_ratio: None, hit_pct: 0, recent_actions: Vec::new(), tk_window_stats: None,
        });
    }

    let ctx = routes::PageCtx::new("Agent Console", "agents", &headers, &token);
    let template = AgentsTemplate {
        title: ctx.title, tenant: ctx.tenant, current_page: ctx.current_page,
        flash: ctx.flash, csrf: ctx.csrf, agents,
    };
    template.render().map(|html| {
        (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
    }).unwrap_or_else(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    })
}
