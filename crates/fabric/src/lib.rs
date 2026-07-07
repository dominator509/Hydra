pub mod egress;
pub mod error;
pub mod mcp;
pub mod rest;
pub mod services;

use axum::Router;

pub use error::{FabricError, ProblemJson};
pub use services::{
    AppState, AutonomyCellDto, AutonomyService, BlastRadiusDto, BridgeService,
    EntityDeleteResponse, EntityService, EnvelopeCreateRequest, EnvelopeService,
    StoreAutonomyService, StoreEntityService, StoreEnvelopeService, StoreTkStatsService,
    TkRouteStat, TkStatsService, TkWindowStats,
};

pub fn app(state: AppState) -> Router {
    rest::router(state)
}
