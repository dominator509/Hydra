mod autonomy;
mod bridges;
mod concierge;
mod entities;
mod envelopes;
mod openapi;
mod tk;

use axum::routing::{get, post};
use axum::Router;

use crate::mcp;
use crate::services::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/openapi.json", get(openapi::openapi))
        .route("/mcp", post(mcp::mcp_route))
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
        .with_state(state)
}
