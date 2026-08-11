use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use axum::http::request::Parts;
use governor::{ActionEnvelope, EnvelopeState};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, JsonObject,
    ListToolsResult, MetaObject, PaginatedRequestParams, ProtocolVersion, ServerCapabilities,
    ServerInfo, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::PrincipalContext;
use crate::capabilities::{CapabilityCategory, CapabilityDescriptor, IdempotencySemantics};
use crate::error::FabricError;
use crate::services::{AppState, BlastRadiusDto, EnvelopeCreateRequest, GovernedExternalProposal};

pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

pub type HydraMcpHttpService = StreamableHttpService<HydraMcpServer, LocalSessionManager>;

#[derive(Clone)]
pub struct HydraMcpServer {
    state: AppState,
}

impl HydraMcpServer {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl ServerHandler for HydraMcpServer {
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Owned(vec![ProtocolVersion::V_2025_11_25])
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .with_server_info(
                Implementation::new("hydra-nexus-control-plane", env!("CARGO_PKG_VERSION"))
                    .with_title("Hydra CRM Control Plane")
                    .with_description(
                        "Authenticated canonical CRM reads and governed action proposals",
                    ),
            )
            .with_instructions(
                "Tenant authority comes only from the authenticated external binding. Tool metadata and HTTP tenant headers are never authority.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let principal = principal_from_context(&context)?;
        let discovery = self
            .state
            .capabilities
            .get("hydra.capabilities.list")
            .ok_or_else(|| ErrorData::internal_error("capability registry unavailable", None))?;
        self.state
            .authorization
            .authorize_external_capability(&principal, discovery, principal.hydra_tenant_id)
            .map_err(|_| ErrorData::invalid_params("capability discovery denied", None))?;

        let tools = self
            .state
            .capabilities
            .descriptors()
            .into_iter()
            .map(|descriptor| tool_from_descriptor(descriptor, None))
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.state
            .capabilities
            .resolve(name)
            .map(|(descriptor, deprecated)| {
                tool_from_descriptor(descriptor, deprecated.then_some(name))
            })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let principal = principal_from_context(&context)?;
        if self
            .state
            .capabilities
            .resolve(request.name.as_ref())
            .is_none()
        {
            return Err(ErrorData::invalid_params("unknown Hydra capability", None));
        }

        let requested_name = request.name.into_owned();
        let arguments = request.arguments.unwrap_or_default();
        let cancellation = context.ct.clone();
        let execution = execute_capability(&self.state, &principal, &requested_name, arguments);
        let outcome = tokio::select! {
            () = cancellation.cancelled() => {
                return Ok(CallToolResult::structured_error(json!({
                    "error": {
                        "code": "request_cancelled",
                        "message": "The MCP request was cancelled"
                    }
                })).into());
            }
            outcome = execution => outcome,
        };

        let result = match outcome {
            Ok(value) => {
                let mut result = CallToolResult::structured(value);
                if self
                    .state
                    .capabilities
                    .resolve(&requested_name)
                    .is_some_and(|(_, deprecated)| deprecated)
                {
                    let mut meta = Map::new();
                    meta.insert("io.hydra/deprecated-alias".to_owned(), Value::Bool(true));
                    result = result.with_meta(Some(MetaObject(meta)));
                }
                result
            }
            Err(error) => structured_tool_error(error),
        };
        Ok(result.into())
    }
}

pub fn streamable_http_service(state: AppState) -> HydraMcpHttpService {
    let control = state.nexus_control_plane.clone();
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_allowed_hosts(control.allowed_hosts.clone())
        .with_allowed_origins(control.allowed_origins.clone())
        .with_max_request_body_bytes(control.max_request_body_bytes);
    let service_state = state.clone();
    StreamableHttpService::new(
        move || Ok(HydraMcpServer::new(service_state.clone())),
        Default::default(),
        config,
    )
}

pub fn tool_schema() -> Value {
    let registry = crate::capabilities::CapabilityRegistry::default();
    let tools = registry
        .descriptors()
        .into_iter()
        .map(|descriptor| {
            serde_json::to_value(tool_from_descriptor(descriptor, None))
                .expect("validated capability tool must serialize")
        })
        .collect::<Vec<_>>();
    json!({
        "protocol": "mcp",
        "version": MCP_PROTOCOL_VERSION,
        "tools": tools
    })
}

pub(crate) async fn execute_capability(
    state: &AppState,
    principal: &PrincipalContext,
    requested_name: &str,
    arguments: JsonObject,
) -> Result<Value, FabricError> {
    let (descriptor, _) = state
        .capabilities
        .resolve(requested_name)
        .ok_or_else(|| FabricError::ValidationFailed("unknown Hydra capability".to_owned()))?;
    state.authorization.authorize_external_capability(
        principal,
        descriptor,
        principal.hydra_tenant_id,
    )?;

    let arguments = Value::Object(arguments);
    match descriptor.name.as_str() {
        "hydra.capabilities.list" => Ok(json!({
            "capabilities": state.capabilities.descriptors()
        })),
        "hydra.crm.context" => compact_context(state, principal).await,
        "hydra.crm.get" => get_entity(state, principal, arguments).await,
        "hydra.crm.pipeline_summary" => pipeline_summary(state, principal, arguments).await,
        "hydra.crm.propose_action" => propose_action(state, principal, descriptor, arguments).await,
        "hydra.crm.search" => search_entities(state, principal, arguments).await,
        "hydra.envelopes.get" => get_envelope(state, principal, arguments).await,
        "hydra.envelopes.list" => list_envelopes(state, principal, arguments).await,
        _ => Err(FabricError::CapabilityUnavailable(
            descriptor
                .unavailable_reason
                .clone()
                .unwrap_or_else(|| "capability has no runtime handler".to_owned()),
        )),
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProposeActionInput {
    deal_id: Uuid,
    stage: String,
    rationale: String,
    idempotency_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    objective_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
}

async fn propose_action(
    state: &AppState,
    principal: &PrincipalContext,
    descriptor: &CapabilityDescriptor,
    arguments: Value,
) -> Result<Value, FabricError> {
    let input: ProposeActionInput = parse_input(arguments)?;
    validate_proposal_input(&input)?;
    let request_hash = canonical_request_hash(&input)?;
    let envelope = state
        .envelopes
        .propose_external(
            principal,
            descriptor,
            GovernedExternalProposal {
                request: EnvelopeCreateRequest {
                    domain: "pipeline".to_owned(),
                    action: "move_stage".to_owned(),
                    kind: Some("deal".to_owned()),
                    targets: vec![input.deal_id],
                    payload: json!({ "stage": input.stage }),
                    rationale: input.rationale,
                    reversal: governor::Reversal::Compensating,
                    blast: BlastRadiusDto {
                        entities: 1,
                        external_sends: 0,
                        money_cents: 0,
                        pii_egress: false,
                    },
                },
                idempotency_key: input.idempotency_key,
                request_hash,
                objective_id: input.objective_id,
                task_id: input.task_id,
            },
        )
        .await?;

    Ok(json!({
        "envelope_id": envelope.id,
        "state": envelope_state_name(envelope.state),
        "decision": decision_name(envelope.state)
    }))
}

fn validate_proposal_input(input: &ProposeActionInput) -> Result<(), FabricError> {
    let optional_ids_valid = [input.objective_id.as_deref(), input.task_id.as_deref()]
        .into_iter()
        .flatten()
        .all(|value| !value.trim().is_empty() && value.len() <= 200);
    if input.stage.trim().is_empty()
        || input.stage.len() > 200
        || input.rationale.trim().is_empty()
        || input.rationale.len() > 2000
        || input.idempotency_key.trim().is_empty()
        || input.idempotency_key.len() > 200
        || !optional_ids_valid
    {
        return Err(FabricError::ValidationFailed(
            "invalid governed stage-change proposal".to_owned(),
        ));
    }
    Ok(())
}

fn canonical_request_hash(input: &ProposeActionInput) -> Result<String, FabricError> {
    let bytes = serde_json::to_vec(input)
        .map_err(|error| FabricError::Internal(format!("serialize proposal hash: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) async fn compact_context(
    state: &AppState,
    principal: &PrincipalContext,
) -> Result<Value, FabricError> {
    let pipeline = pipeline_summary(state, principal, json!({})).await?;
    let pending_approvals = state
        .envelopes
        .list(principal.hydra_tenant_id, EnvelopeState::PendingApproval)
        .await?
        .len();
    let capability_availability = state
        .capabilities
        .descriptors()
        .into_iter()
        .map(|descriptor| {
            json!({
                "name": descriptor.name,
                "version": descriptor.version,
                "available": descriptor.available,
                "unavailable_reason": descriptor.unavailable_reason
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "tenant": {
            "hydra_tenant_id": principal.hydra_tenant_id,
            "binding_id": principal.binding_id,
            "external_tenant_id": principal.external_tenant_id,
            "external_business_id": principal.external_business_id
        },
        "pipeline": pipeline,
        "pending_approvals": pending_approvals,
        "bridge_health": [{
            "available": false,
            "reason": "tenant bridge inventory is not exposed until runtime bridge wiring is verified in EP-013"
        }],
        "capability_availability": capability_availability
    }))
}

fn tool_from_descriptor(descriptor: &CapabilityDescriptor, deprecated_alias: Option<&str>) -> Tool {
    let mut tool = Tool::default();
    tool.name = Cow::Owned(
        deprecated_alias
            .unwrap_or(descriptor.name.as_str())
            .to_owned(),
    );
    tool.title = Some(capability_title(&descriptor.name));
    tool.description = Some(Cow::Owned(match deprecated_alias {
        Some(_) => format!(
            "DEPRECATED alias for {}. {}",
            descriptor.name, descriptor.description
        ),
        None => descriptor.description.clone(),
    }));
    tool.input_schema = schema_object(&descriptor.input_schema);
    tool.output_schema = Some(schema_object(&descriptor.output_schema));

    let read_only = descriptor.category == CapabilityCategory::Query;
    let mut annotations = ToolAnnotations::default();
    annotations.title = tool.title.clone();
    annotations.read_only_hint = Some(read_only);
    annotations.destructive_hint = Some(false);
    annotations.idempotent_hint = Some(matches!(
        descriptor.idempotency,
        IdempotencySemantics::NaturallyIdempotent | IdempotencySemantics::RequiredKey
    ));
    annotations.open_world_hint = Some(false);
    tool.annotations = Some(annotations);

    let mut meta = Map::new();
    meta.insert(
        "io.hydra/capability".to_owned(),
        json!({
            "canonicalName": descriptor.name,
            "version": descriptor.version,
            "available": descriptor.available,
            "unavailableReason": descriptor.unavailable_reason,
            "requiredScopes": descriptor.required_scopes,
            "riskClass": descriptor.risk_class,
            "executionMode": descriptor.execution_mode,
            "deprecatedAlias": deprecated_alias.is_some()
        }),
    );
    tool.meta = Some(MetaObject(meta));
    tool
}

fn schema_object(schema: &Value) -> Arc<JsonObject> {
    Arc::new(schema.as_object().cloned().unwrap_or_else(|| {
        Map::from_iter([
            ("type".to_owned(), Value::String("object".to_owned())),
            ("additionalProperties".to_owned(), Value::Bool(false)),
        ])
    }))
}

fn capability_title(name: &str) -> String {
    name.split('.')
        .skip(1)
        .map(|part| {
            part.split('_')
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" - ")
}

fn principal_from_context(
    context: &RequestContext<RoleServer>,
) -> Result<PrincipalContext, ErrorData> {
    context
        .extensions
        .get::<Parts>()
        .and_then(|parts| parts.extensions.get::<PrincipalContext>())
        .cloned()
        .ok_or_else(|| {
            ErrorData::internal_error("authenticated principal context unavailable", None)
        })
}

fn structured_tool_error(error: FabricError) -> CallToolResult {
    let detail = match &error {
        FabricError::ValidationFailed(detail) | FabricError::CapabilityUnavailable(detail) => {
            Some(detail.clone())
        }
        _ => None,
    };
    CallToolResult::structured_error(json!({
        "error": {
            "code": error.code(),
            "message": error.title(),
            "detail": detail
        }
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetEntityInput {
    kind: String,
    entity_id: Uuid,
}

async fn get_entity(
    state: &AppState,
    principal: &PrincipalContext,
    arguments: Value,
) -> Result<Value, FabricError> {
    let input: GetEntityInput = parse_input(arguments)?;
    validate_kind(&input.kind)?;
    let entity = state
        .entities
        .get(principal.hydra_tenant_id, &input.kind, input.entity_id)
        .await?;
    serde_json::to_value(entity).map_err(|error| FabricError::Internal(error.to_string()))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchInput {
    query: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default = "default_search_limit")]
    limit: u16,
}

fn default_search_limit() -> u16 {
    20
}

async fn search_entities(
    state: &AppState,
    principal: &PrincipalContext,
    arguments: Value,
) -> Result<Value, FabricError> {
    let input: SearchInput = parse_input(arguments)?;
    let query = input.query.trim().to_lowercase();
    if query.is_empty() || query.len() > 500 || input.limit == 0 || input.limit > 100 {
        return Err(FabricError::ValidationFailed(
            "query must be 1..500 bytes and limit must be 1..100".to_owned(),
        ));
    }

    let kinds = match input.kind {
        Some(kind) => {
            validate_kind(&kind)?;
            vec![kind]
        }
        None => cdm::builtin_kind_names()
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };
    let mut matches = Vec::new();
    for kind in kinds {
        let remaining = usize::from(input.limit).saturating_sub(matches.len());
        if remaining == 0 {
            break;
        }
        let entities = state
            .entities
            .list(principal.hydra_tenant_id, &kind, None, 200)
            .await?;
        for entity in entities {
            let searchable = format!(
                "{} {} {} {}",
                entity.id,
                entity.kind,
                entity.origin_ref.as_deref().unwrap_or_default(),
                serde_json::to_string(&entity.body).unwrap_or_default()
            )
            .to_lowercase();
            if searchable.contains(&query) {
                matches.push(entity);
                if matches.len() == usize::from(input.limit) {
                    break;
                }
            }
        }
    }
    Ok(json!({ "items": matches }))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PipelineSummaryInput {
    #[serde(default)]
    pipeline_id: Option<String>,
}

async fn pipeline_summary(
    state: &AppState,
    principal: &PrincipalContext,
    arguments: Value,
) -> Result<Value, FabricError> {
    let input: PipelineSummaryInput = parse_input(arguments)?;
    if input
        .pipeline_id
        .as_ref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(FabricError::ValidationFailed(
            "pipeline_id cannot be blank".to_owned(),
        ));
    }

    let deals = state
        .entities
        .list(principal.hydra_tenant_id, "deal", None, 5_001)
        .await?;
    if deals.len() > 5_000 {
        return Err(FabricError::CapabilityUnavailable(
            "pipeline summary exceeds the bounded 5000-deal projection".to_owned(),
        ));
    }

    let mut stages = BTreeMap::<String, (u64, u64)>::new();
    let mut total_deals = 0_u64;
    for deal in deals {
        if input.pipeline_id.as_ref().is_some_and(|pipeline_id| {
            deal.body
                .get("pipeline_id")
                .and_then(Value::as_str)
                .is_none_or(|actual| actual != pipeline_id)
        }) {
            continue;
        }
        let stage_id = deal
            .body
            .get("stage_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("unassigned")
            .to_owned();
        let amount_cents = deal
            .body
            .get("amount_cents")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let stage = stages.entry(stage_id).or_default();
        stage.0 = stage.0.saturating_add(1);
        stage.1 = stage.1.saturating_add(amount_cents);
        total_deals = total_deals.saturating_add(1);
    }

    let stages = stages
        .into_iter()
        .map(|(stage_id, (deal_count, amount_cents))| {
            json!({
                "stage_id": stage_id,
                "deal_count": deal_count,
                "amount_cents": amount_cents
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "total_deals": total_deals,
        "stages": stages
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListEnvelopesInput {
    #[serde(default = "default_envelope_state")]
    state: String,
    #[serde(default = "default_envelope_limit")]
    limit: u16,
}

fn default_envelope_state() -> String {
    "pending_approval".to_owned()
}

fn default_envelope_limit() -> u16 {
    20
}

async fn list_envelopes(
    state: &AppState,
    principal: &PrincipalContext,
    arguments: Value,
) -> Result<Value, FabricError> {
    let input: ListEnvelopesInput = parse_input(arguments)?;
    if input.limit == 0 || input.limit > 100 {
        return Err(FabricError::ValidationFailed(
            "limit must be 1..100".to_owned(),
        ));
    }
    let envelope_state = parse_envelope_state(&input.state)?;
    let envelopes = state
        .envelopes
        .list(principal.hydra_tenant_id, envelope_state)
        .await?
        .into_iter()
        .take(usize::from(input.limit))
        .map(envelope_projection)
        .collect::<Vec<_>>();
    Ok(json!({ "envelopes": envelopes }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetEnvelopeInput {
    envelope_id: Uuid,
}

async fn get_envelope(
    state: &AppState,
    principal: &PrincipalContext,
    arguments: Value,
) -> Result<Value, FabricError> {
    let input: GetEnvelopeInput = parse_input(arguments)?;
    let envelope = state
        .envelopes
        .get(principal.hydra_tenant_id, input.envelope_id)
        .await?;
    Ok(envelope_projection(envelope))
}

fn envelope_projection(envelope: ActionEnvelope) -> Value {
    json!({
        "id": envelope.id,
        "domain": envelope.domain,
        "action": envelope.action,
        "kind": envelope.kind,
        "targets": envelope.targets,
        "state": envelope.state,
        "reversal": envelope.reversal,
        "blast": envelope.blast,
        "invocation": envelope.invocation,
        "history": envelope.history
    })
}

fn envelope_state_name(state: EnvelopeState) -> &'static str {
    match state {
        EnvelopeState::Proposed => "proposed",
        EnvelopeState::PendingApproval => "pending_approval",
        EnvelopeState::Approved => "approved",
        EnvelopeState::Executing => "executing",
        EnvelopeState::Executed => "executed",
        EnvelopeState::Failed => "failed",
        EnvelopeState::RolledBack => "rolled_back",
        EnvelopeState::Rejected => "rejected",
    }
}

fn decision_name(state: EnvelopeState) -> &'static str {
    match state {
        EnvelopeState::Proposed => "suggest",
        EnvelopeState::PendingApproval => "queue",
        EnvelopeState::Approved
        | EnvelopeState::Executing
        | EnvelopeState::Executed
        | EnvelopeState::Failed
        | EnvelopeState::RolledBack => "execute",
        EnvelopeState::Rejected => "rejected",
    }
}

fn parse_envelope_state(raw: &str) -> Result<EnvelopeState, FabricError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "proposed" => Ok(EnvelopeState::Proposed),
        "pendingapproval" | "pending_approval" => Ok(EnvelopeState::PendingApproval),
        "approved" => Ok(EnvelopeState::Approved),
        "executing" => Ok(EnvelopeState::Executing),
        "executed" => Ok(EnvelopeState::Executed),
        "failed" => Ok(EnvelopeState::Failed),
        "rolledback" | "rolled_back" => Ok(EnvelopeState::RolledBack),
        "rejected" => Ok(EnvelopeState::Rejected),
        _ => Err(FabricError::ValidationFailed(
            "unknown envelope state".to_owned(),
        )),
    }
}

fn parse_input<T: for<'de> Deserialize<'de>>(arguments: Value) -> Result<T, FabricError> {
    serde_json::from_value(arguments)
        .map_err(|error| FabricError::ValidationFailed(error.to_string()))
}

fn validate_kind(kind: &str) -> Result<(), FabricError> {
    if cdm::builtin_kind_names().contains(&kind) {
        Ok(())
    } else {
        Err(FabricError::ValidationFailed(
            "unknown canonical entity kind".to_owned(),
        ))
    }
}
