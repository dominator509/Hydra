pub mod auth;
pub mod capabilities;
pub mod egress;
pub mod error;
pub mod mcp;
pub mod middleware;
pub mod rate;
pub mod rest;
pub mod services;
pub mod trace_context;
pub mod wiring;

use axum::Router;

pub use auth::{
    parse_algorithm, AuthCtx, AuthorizationService, CorrelationContext, ExternalBindingResolver,
    NexusOidcConfig, OidcAuthenticator, OidcKeySource, PrincipalContext, PrincipalType,
    ResolvedExternalBinding, Role, Scope, Session, SessionStore,
};
pub use capabilities::{
    CapabilityCategory, CapabilityDescriptor, CapabilityRegistry, CapabilityRegistryError,
    ExecutionMode, GovernorBinding, IdempotencySemantics, ReversalSemantics, RiskClass,
};
pub use error::{FabricError, ProblemJson};
pub use services::{
    AppState, AutonomyCellDto, AutonomyService, BlastRadiusDto, BridgeGrantDto,
    BridgeRegisterRequest, BridgeService, BridgeStatusDto, ConciergePingResponse, ConciergeService,
    ConciergeServiceImpl, EntityDeleteResponse, EntityService, EnvelopeApprovalDecision,
    EnvelopeApprovalReceipt, EnvelopeApprovalRequest, EnvelopeCreateRequest, EnvelopeService,
    EventInfrastructureStatus, EventStatusService, ExecutionDispatcher, GovernedExternalProposal,
    GovernorProvider, NexusControlPlaneConfig, StoreAutonomyService, StoreBridgeService,
    StoreEntityService, StoreEnvelopeService, StoreTkStatsService, TkRouteStat, TkStatsService,
    TkWindowStats,
};

pub fn app(state: AppState) -> Router {
    rest::router(state)
}
