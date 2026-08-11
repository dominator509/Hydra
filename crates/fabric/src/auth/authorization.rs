use std::collections::BTreeSet;
use std::sync::Arc;

use uuid::Uuid;

use super::{AuthCtx, PrincipalContext, PrincipalType, Role, Scope};
use crate::capabilities::CapabilityDescriptor;
use crate::FabricError;

#[derive(Debug, Clone, Default)]
pub struct AuthorizationService {
    sufficient_approval_strengths: Arc<BTreeSet<String>>,
}

impl AuthorizationService {
    pub fn new(sufficient_approval_strengths: impl IntoIterator<Item = String>) -> Self {
        Self {
            sufficient_approval_strengths: Arc::new(
                sufficient_approval_strengths
                    .into_iter()
                    .filter(|value| !value.trim().is_empty())
                    .collect(),
            ),
        }
    }

    pub fn authorize_external_capability(
        &self,
        principal: &PrincipalContext,
        capability: &CapabilityDescriptor,
        hydra_tenant_id: Uuid,
    ) -> Result<(), FabricError> {
        self.authorize_external_scopes(
            principal,
            capability.required_scopes.iter().copied(),
            hydra_tenant_id,
        )?;
        if !capability.available {
            return Err(FabricError::CapabilityUnavailable(
                capability
                    .unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "capability is not available".to_owned()),
            ));
        }
        Ok(())
    }

    pub fn authorize_local_capability(
        &self,
        ctx: &AuthCtx,
        capability: &CapabilityDescriptor,
        hydra_tenant_id: Uuid,
    ) -> Result<(), FabricError> {
        self.authorize_local_scopes(
            ctx,
            capability.required_scopes.iter().copied(),
            hydra_tenant_id,
        )?;
        if !capability.available {
            return Err(FabricError::CapabilityUnavailable(
                capability
                    .unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "capability is not available".to_owned()),
            ));
        }
        Ok(())
    }

    pub fn authorize_external_scope(
        &self,
        principal: &PrincipalContext,
        scope: Scope,
        hydra_tenant_id: Uuid,
    ) -> Result<(), FabricError> {
        self.authorize_external_scopes(principal, [scope], hydra_tenant_id)
    }

    pub fn authorize_local_scope(
        &self,
        ctx: &AuthCtx,
        scope: Scope,
        hydra_tenant_id: Uuid,
    ) -> Result<(), FabricError> {
        self.authorize_local_scopes(ctx, [scope], hydra_tenant_id)
    }

    pub fn authorize_external_approval(
        &self,
        principal: &PrincipalContext,
        hydra_tenant_id: Uuid,
        proposer_principal_id: &str,
    ) -> Result<(), FabricError> {
        self.authorize_external_scope(principal, Scope::EnvelopesApprove, hydra_tenant_id)?;
        if principal.principal_type != PrincipalType::Human
            || principal.delegated_by.as_deref().is_none_or(str::is_empty)
            || principal.principal_id == proposer_principal_id
            || principal
                .authentication_strength
                .as_ref()
                .is_none_or(|strength| !self.sufficient_approval_strengths.contains(strength))
        {
            return Err(FabricError::AuthzDenied);
        }
        Ok(())
    }

    pub fn scopes_for_roles(roles: &[Role]) -> BTreeSet<Scope> {
        let mut scopes = BTreeSet::new();
        if roles.iter().any(|role| role.require(&Role::Viewer)) {
            scopes.extend([
                Scope::CapabilitiesRead,
                Scope::CrmRead,
                Scope::CrmContextRead,
                Scope::EnvelopesRead,
                Scope::BridgesRead,
                Scope::AutonomyRead,
            ]);
        }
        if roles.iter().any(|role| role.require(&Role::Operator)) {
            scopes.insert(Scope::CrmPropose);
        }
        if roles.iter().any(|role| role.require(&Role::Approver)) {
            scopes.insert(Scope::EnvelopesApprove);
        }
        if roles.iter().any(|role| role.require(&Role::Admin)) {
            scopes.extend([Scope::BridgesAdmin, Scope::AutonomyAdmin]);
        }
        scopes
    }

    fn authorize_external_scopes(
        &self,
        principal: &PrincipalContext,
        required_scopes: impl IntoIterator<Item = Scope>,
        hydra_tenant_id: Uuid,
    ) -> Result<(), FabricError> {
        if !principal.is_external()
            || principal.binding_id.is_none()
            || principal.external_tenant_id.is_none()
            || principal.external_business_id.is_none()
            || principal.hydra_tenant_id != hydra_tenant_id
            || hydra_tenant_id.is_nil()
            || required_scopes
                .into_iter()
                .any(|scope| !principal.has_scope(scope))
        {
            return Err(FabricError::AuthzDenied);
        }
        Ok(())
    }

    fn authorize_local_scopes(
        &self,
        ctx: &AuthCtx,
        required_scopes: impl IntoIterator<Item = Scope>,
        hydra_tenant_id: Uuid,
    ) -> Result<(), FabricError> {
        let session = ctx.session.as_ref().ok_or(FabricError::AuthzDenied)?;
        if ctx.tenant != hydra_tenant_id
            || session.tenant_id != hydra_tenant_id
            || hydra_tenant_id.is_nil()
        {
            return Err(FabricError::AuthzDenied);
        }
        let granted = Self::scopes_for_roles(&session.roles);
        if required_scopes
            .into_iter()
            .any(|scope| !granted.contains(&scope))
        {
            return Err(FabricError::AuthzDenied);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapabilityRegistry, CorrelationContext};

    #[test]
    fn capability_authorization_denies_read_only_principal_before_unavailable_disclosure() {
        let registry = CapabilityRegistry::default();
        let proposal = registry
            .get("hydra.crm.propose_action")
            .expect("proposal descriptor");
        let tenant = Uuid::new_v4();
        let principal = principal(
            tenant,
            PrincipalType::NexusService,
            BTreeSet::from([Scope::CrmRead]),
        );
        let result = AuthorizationService::default()
            .authorize_external_capability(&principal, proposal, tenant);
        assert!(matches!(result, Err(FabricError::AuthzDenied)));
    }

    #[test]
    fn capability_authorization_reports_unavailable_after_scope_passes() {
        let registry = CapabilityRegistry::default();
        let proposal = registry
            .get("hydra.crm.propose_action")
            .expect("proposal descriptor");
        let tenant = Uuid::new_v4();
        let principal = principal(
            tenant,
            PrincipalType::NexusService,
            BTreeSet::from([Scope::CrmPropose]),
        );
        let result = AuthorizationService::default()
            .authorize_external_capability(&principal, proposal, tenant);
        assert!(matches!(result, Err(FabricError::CapabilityUnavailable(_))));
    }

    #[test]
    fn capability_authorization_blocks_cross_tenant_principal() {
        let registry = CapabilityRegistry::default();
        let search = registry.get("hydra.crm.search").expect("search descriptor");
        let principal = principal(
            Uuid::new_v4(),
            PrincipalType::NexusService,
            BTreeSet::from([Scope::CrmRead]),
        );
        let result = AuthorizationService::default().authorize_external_capability(
            &principal,
            search,
            Uuid::new_v4(),
        );
        assert!(matches!(result, Err(FabricError::AuthzDenied)));
    }

    #[test]
    fn capability_authorization_agent_cannot_approve() {
        let tenant = Uuid::new_v4();
        let mut principal = principal(
            tenant,
            PrincipalType::NexusAgent,
            BTreeSet::from([Scope::EnvelopesApprove]),
        );
        principal.delegated_by = Some("human:owner".to_owned());
        principal.authentication_strength = Some("mfa".to_owned());
        let authorization = AuthorizationService::new(["mfa".to_owned()]);
        assert!(matches!(
            authorization.authorize_external_approval(&principal, tenant, "agent:proposer"),
            Err(FabricError::AuthzDenied)
        ));
    }

    #[test]
    fn capability_authorization_human_cannot_approve_own_proposal() {
        let tenant = Uuid::new_v4();
        let mut principal = principal(
            tenant,
            PrincipalType::Human,
            BTreeSet::from([Scope::EnvelopesApprove]),
        );
        principal.delegated_by = Some("nexus:user-session".to_owned());
        principal.authentication_strength = Some("mfa".to_owned());
        let authorization = AuthorizationService::new(["mfa".to_owned()]);
        assert!(matches!(
            authorization.authorize_external_approval(&principal, tenant, &principal.principal_id),
            Err(FabricError::AuthzDenied)
        ));
    }

    #[test]
    fn capability_authorization_distinct_human_with_configured_strength_passes() {
        let tenant = Uuid::new_v4();
        let mut principal = principal(
            tenant,
            PrincipalType::Human,
            BTreeSet::from([Scope::EnvelopesApprove]),
        );
        principal.delegated_by = Some("nexus:user-session".to_owned());
        principal.authentication_strength = Some("mfa".to_owned());
        let authorization = AuthorizationService::new(["mfa".to_owned()]);
        assert!(authorization
            .authorize_external_approval(&principal, tenant, "human:proposer")
            .is_ok());
    }

    #[test]
    fn capability_authorization_local_roles_map_to_stable_scopes() {
        let viewer = AuthorizationService::scopes_for_roles(&[Role::Viewer]);
        assert!(viewer.contains(&Scope::CrmRead));
        assert!(!viewer.contains(&Scope::CrmPropose));

        let approver = AuthorizationService::scopes_for_roles(&[Role::Approver]);
        assert!(approver.contains(&Scope::CrmPropose));
        assert!(approver.contains(&Scope::EnvelopesApprove));
        assert!(!approver.contains(&Scope::BridgesAdmin));

        let admin = AuthorizationService::scopes_for_roles(&[Role::Admin]);
        assert!(admin.contains(&Scope::EnvelopesApprove));
        assert!(admin.contains(&Scope::BridgesAdmin));
        assert!(admin.contains(&Scope::AutonomyAdmin));
    }

    fn principal(
        tenant: Uuid,
        principal_type: PrincipalType,
        scopes: BTreeSet<Scope>,
    ) -> PrincipalContext {
        PrincipalContext {
            principal_id: "principal:test".to_owned(),
            principal_type,
            hydra_tenant_id: tenant,
            external_provider: Some("nexus".to_owned()),
            external_tenant_id: Some("external-tenant".to_owned()),
            external_business_id: Some("business".to_owned()),
            scopes,
            delegated_by: None,
            authentication_strength: None,
            token_id: Some("token-id".to_owned()),
            correlation: CorrelationContext {
                request_id: "request".to_owned(),
                correlation_id: "correlation".to_owned(),
                causation_id: None,
            },
            binding_id: Some(Uuid::new_v4()),
            trace: store::TraceContext::fresh(),
        }
    }
}
