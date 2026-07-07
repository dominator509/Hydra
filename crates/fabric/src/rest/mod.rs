mod entities;
mod envelopes;
mod openapi;
mod tk;

use axum::routing::{get, post};
use axum::Router;

use crate::services::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/openapi.json", get(openapi::openapi))
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
