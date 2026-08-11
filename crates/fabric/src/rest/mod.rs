mod autonomy;
mod bridges;
mod concierge;
mod entities;
mod envelopes;
mod nexus;
mod oauth;
mod openapi;
mod tk;

use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use uuid::Uuid;

use crate::auth::{AuthCtx, Role, Session};
use crate::error::FabricError;
use crate::mcp;
use crate::services::{tenant_from_headers, AppState};

pub use nexus::propose_stage_change;

pub fn router(state: AppState) -> Router {
    let public = Router::new()
        .route("/v1/openapi.json", get(openapi::openapi))
        .route("/oauth/token", post(oauth::token_endpoint))
        .route(
            "/.well-known/oauth-protected-resource",
            get(nexus::protected_resource_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(nexus::protected_resource_metadata),
        )
        .with_state(state.clone());

    let local_api = Router::new()
        .route(
            "/v1/autonomy/cells",
            get(autonomy::list_cells).put(autonomy::replace_cells),
        )
        .route("/v1/bridges", post(bridges::register_bridge))
        .route("/v1/bridges/:id/status", get(bridges::bridge_status))
        .route("/v1/bridges/:id/pause", post(bridges::pause_bridge))
        .route("/v1/bridges/:id/resume", post(bridges::resume_bridge))
        .route("/v1/concierge/ping", post(concierge::concierge_ping))
        .route(
            "/v1/entities/:kind",
            get(entities::list_entities).post(entities::create_entity),
        )
        .route(
            "/v1/entities/:kind/:id",
            get(entities::get_entity)
                .patch(entities::patch_entity)
                .delete(entities::delete_entity),
        )
        .route(
            "/v1/envelopes",
            get(envelopes::list_envelopes).post(envelopes::propose_envelope),
        )
        .route(
            "/v1/envelopes/:id/approve",
            post(envelopes::approve_envelope),
        )
        .route("/v1/envelopes/:id/reject", post(envelopes::reject_envelope))
        .route("/v1/tk/ledger", get(tk::ledger_window))
        .route_layer(from_fn_with_state(state.clone(), local_auth_middleware))
        .with_state(state.clone());

    let nexus_api = Router::new()
        .route("/v1/nexus/capabilities", get(nexus::capabilities))
        .route("/v1/nexus/context", get(nexus::context))
        .route(
            "/v1/nexus/proposals/stage-change",
            post(nexus::propose_stage_change),
        )
        .route(
            "/v1/nexus/envelopes/:id/approval",
            post(nexus::decide_envelope_approval),
        )
        .route("/v1/nexus/bindings", get(nexus::bindings))
        .route("/v1/nexus/events/status", get(nexus::events_status))
        .route_layer(from_fn_with_state(
            state.clone(),
            nexus::external_auth_middleware,
        ))
        .with_state(state.clone());

    let mcp_api = Router::new()
        .nest_service("/mcp", mcp::streamable_http_service(state.clone()))
        .route_layer(from_fn_with_state(
            state.clone(),
            nexus::external_auth_middleware,
        ));

    public.merge(local_api).merge(nexus_api).merge(mcp_api)
}

async fn local_auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let tenant = match tenant_from_headers(request.headers()) {
        Ok(tenant) if !tenant.is_nil() => tenant,
        Ok(_) => return FabricError::AuthzDenied.into_response(),
        Err(error) => return error.into_response(),
    };
    let Some(token) = local_token(request.headers()).map(str::to_owned) else {
        return FabricError::AuthzDenied.into_response();
    };
    if token.is_empty() || token.len() > 512 {
        return FabricError::AuthzDenied.into_response();
    }

    let session = if token == "hydra-dev-admin" && state.allow_development_identity {
        Session {
            user_id: Uuid::nil(),
            tenant_id: tenant,
            username: "admin".to_owned(),
            roles: vec![Role::Admin],
            token,
        }
    } else {
        match state.auth.lookup(&token).await {
            Ok(Some(session)) if session.tenant_id == tenant => session,
            Ok(_) => return FabricError::AuthzDenied.into_response(),
            Err(error) => return error.into_response(),
        }
    };

    let context = AuthCtx {
        principal: format!("user:{}", session.username),
        tenant,
        session: Some(session),
    };
    if let Err(error) = context.require_role(Role::Viewer) {
        return error.into_response();
    }
    let rate_key = format!("local:{}:{}", tenant, context.principal);
    if let Err(error) = state.rate_limiter.check(&rate_key) {
        return error.into_response();
    }
    request.extensions_mut().insert(context);
    next.run(request).await
}

fn local_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    if let Some(token) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    {
        return Some(token);
    }
    headers
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|cookie| {
            cookie
                .split(';')
                .map(str::trim)
                .find_map(|pair| pair.strip_prefix("hydra-session="))
        })
}
