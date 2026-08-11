use std::collections::BTreeSet;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::FabricError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalType {
    Human,
    NexusService,
    NexusAgent,
    HydraInternalAgent,
    LocalHydraUser,
}

impl FromStr for PrincipalType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "human" => Ok(Self::Human),
            "nexus_service" => Ok(Self::NexusService),
            "nexus_agent" => Ok(Self::NexusAgent),
            "hydra_internal_agent" => Ok(Self::HydraInternalAgent),
            "local_hydra_user" => Ok(Self::LocalHydraUser),
            other => Err(format!("unknown principal type '{other}'")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Scope {
    #[serde(rename = "hydra.capabilities.read")]
    CapabilitiesRead,
    #[serde(rename = "hydra.crm.read")]
    CrmRead,
    #[serde(rename = "hydra.crm.context.read")]
    CrmContextRead,
    #[serde(rename = "hydra.crm.propose")]
    CrmPropose,
    #[serde(rename = "hydra.envelopes.read")]
    EnvelopesRead,
    #[serde(rename = "hydra.envelopes.approve")]
    EnvelopesApprove,
    #[serde(rename = "hydra.bridges.read")]
    BridgesRead,
    #[serde(rename = "hydra.bridges.admin")]
    BridgesAdmin,
    #[serde(rename = "hydra.autonomy.read")]
    AutonomyRead,
    #[serde(rename = "hydra.autonomy.admin")]
    AutonomyAdmin,
}

impl Scope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CapabilitiesRead => "hydra.capabilities.read",
            Self::CrmRead => "hydra.crm.read",
            Self::CrmContextRead => "hydra.crm.context.read",
            Self::CrmPropose => "hydra.crm.propose",
            Self::EnvelopesRead => "hydra.envelopes.read",
            Self::EnvelopesApprove => "hydra.envelopes.approve",
            Self::BridgesRead => "hydra.bridges.read",
            Self::BridgesAdmin => "hydra.bridges.admin",
            Self::AutonomyRead => "hydra.autonomy.read",
            Self::AutonomyAdmin => "hydra.autonomy.admin",
        }
    }
}

impl FromStr for Scope {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "hydra.capabilities.read" => Ok(Self::CapabilitiesRead),
            "hydra.crm.read" => Ok(Self::CrmRead),
            "hydra.crm.context.read" => Ok(Self::CrmContextRead),
            "hydra.crm.propose" => Ok(Self::CrmPropose),
            "hydra.envelopes.read" => Ok(Self::EnvelopesRead),
            "hydra.envelopes.approve" => Ok(Self::EnvelopesApprove),
            "hydra.bridges.read" => Ok(Self::BridgesRead),
            "hydra.bridges.admin" => Ok(Self::BridgesAdmin),
            "hydra.autonomy.read" => Ok(Self::AutonomyRead),
            "hydra.autonomy.admin" => Ok(Self::AutonomyAdmin),
            other => Err(format!("unknown Hydra scope '{other}'")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationContext {
    pub request_id: String,
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalContext {
    pub principal_id: String,
    pub principal_type: PrincipalType,
    pub hydra_tenant_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_business_id: Option<String>,
    pub scopes: BTreeSet<Scope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegated_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication_strength: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    pub correlation: CorrelationContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_id: Option<Uuid>,
    #[serde(skip, default)]
    pub trace: store::TraceContext,
}

impl PrincipalContext {
    pub fn has_scope(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }

    pub fn require_scope(&self, scope: Scope) -> Result<(), FabricError> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err(FabricError::AuthzDenied)
        }
    }

    pub fn is_external(&self) -> bool {
        matches!(
            self.principal_type,
            PrincipalType::Human | PrincipalType::NexusService | PrincipalType::NexusAgent
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_round_trip_is_stable() {
        for scope in [
            Scope::CapabilitiesRead,
            Scope::CrmRead,
            Scope::CrmContextRead,
            Scope::CrmPropose,
            Scope::EnvelopesRead,
            Scope::EnvelopesApprove,
            Scope::BridgesRead,
            Scope::BridgesAdmin,
            Scope::AutonomyRead,
            Scope::AutonomyAdmin,
        ] {
            assert_eq!(Scope::from_str(scope.as_str()).ok(), Some(scope));
        }
    }

    #[test]
    fn serialized_principal_never_has_access_token_field() {
        let principal = PrincipalContext {
            principal_id: "nexus-service:test".into(),
            principal_type: PrincipalType::NexusService,
            hydra_tenant_id: Uuid::nil(),
            external_provider: Some("nexus".into()),
            external_tenant_id: Some("tenant-a".into()),
            external_business_id: Some("business-a".into()),
            scopes: BTreeSet::from([Scope::CrmRead]),
            delegated_by: None,
            authentication_strength: None,
            token_id: Some("token-id".into()),
            correlation: CorrelationContext {
                request_id: "request-1".into(),
                correlation_id: "correlation-1".into(),
                causation_id: None,
            },
            binding_id: Some(Uuid::nil()),
            trace: store::TraceContext::fresh(),
        };

        let value = serde_json::to_value(principal).expect("principal serializes");
        assert!(value.get("access_token").is_none());
        assert!(value.get("token").is_none());
    }
}
