pub mod auth;
pub mod egress;
pub mod error;
pub mod mcp;
pub mod rest;
pub mod services;

use axum::Router;

pub use error::{FabricError, ProblemJson};
pub use auth::{AuthCtx, Role, Session, SessionStore};
pub use services::{
    AppState, AutonomyCellDto, AutonomyService, BlastRadiusDto, BridgeGrantDto,
    BridgeRegisterRequest, BridgeService, BridgeStatusDto, ConciergePingResponse, ConciergeService,
    ConciergeServiceImpl, EntityDeleteResponse, EntityService, EnvelopeCreateRequest,
    EnvelopeService, StoreAutonomyService, StoreBridgeService, StoreEntityService,
    StoreEnvelopeService, StoreTkStatsService, TkRouteStat, TkStatsService, TkWindowStats,
};

pub fn app(state: AppState) -> Router {
    rest::router(state)
}
