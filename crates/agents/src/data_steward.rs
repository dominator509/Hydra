//! DataSteward — identity deduplication and merge orchestration.
//!
//! Operates on `cdm::Entity` values, proposes merges based on collision
//! heuristics, and emits governed merge proposals without mutating entities.

use cdm::{proposals, Entity, MergeProposal};
use governor::{ActionEnvelope, BlastRadius, EnvelopeState, InvocationContext, Reversal};
use serde_json::json;
use uuid::Uuid;

use crate::bridge_engineer::AgentError;
use crate::{AgentCapabilityAvailability, AgentCapabilityDescriptor};

/// A deduplication context tied to a tenant.
///
/// The DataSteward holds no mutable state itself — all state is passed
/// through method arguments, making it testable without fixtures.
pub struct DataSteward;

impl DataSteward {
    pub fn capability() -> AgentCapabilityDescriptor {
        AgentCapabilityDescriptor {
            name: "hydra.agent.data_steward.merge_proposal".to_owned(),
            availability: AgentCapabilityAvailability::Experimental,
            envelope_only: true,
            reason: Some(
                "deduplication emits governed data/merge_parties proposals; no execution handler is registered"
                    .to_owned(),
            ),
        }
    }

    /// Propose merges for the given tenant's entities.
    ///
    /// Uses `cdm::proposals` under the hood to generate `MergeProposal` values,
    /// then enriches them with the actual `Entity` data for the caller.
    /// Returns an empty vec when no collisions are found.
    pub fn deduplicate(_tenant: Uuid, entities: &[Entity]) -> Vec<MergeProposal> {
        if entities.len() < 2 {
            return Vec::new();
        }

        let party_views: Vec<cdm::PartyView> = entities
            .iter()
            .map(|e| cdm::PartyView {
                id: e.id,
                display_name: e
                    .body
                    .get("name")
                    .and_then(|v| v.as_str().map(|s| s.to_owned())),
                email: e
                    .body
                    .get("email")
                    .and_then(|v| v.as_str().map(|s| s.to_owned())),
                phone: e
                    .body
                    .get("phone")
                    .and_then(|v| v.as_str().map(|s| s.to_owned())),
                domain: e
                    .body
                    .get("domain")
                    .and_then(|v| v.as_str().map(|s| s.to_owned())),
            })
            .collect();

        proposals(&party_views)
    }

    /// Convert a deduplication candidate into governed work without mutating CDM state.
    pub fn merge(
        proposal: &MergeProposal,
        entities: &[Entity],
    ) -> Result<ActionEnvelope, AgentError> {
        if proposal.ids.len() < 2 {
            return Err(AgentError::Internal(
                "merge requires at least 2 entity ids".into(),
            ));
        }

        let id_set: std::collections::HashSet<Uuid> = proposal.ids.iter().cloned().collect();
        let involved: Vec<&Entity> = entities.iter().filter(|e| id_set.contains(&e.id)).collect();

        if id_set.len() != proposal.ids.len() || involved.len() != proposal.ids.len() {
            return Err(AgentError::Internal(
                "merge proposal ids not found in entity slice".into(),
            ));
        }

        let survivor = involved
            .iter()
            .copied()
            .find(|entity| entity.id == proposal.ids[0])
            .ok_or_else(|| AgentError::Internal("merge survivor was not found".to_owned()))?;
        if survivor.tenant.is_nil()
            || involved
                .iter()
                .any(|entity| entity.tenant != survivor.tenant || entity.kind != "party")
        {
            return Err(AgentError::Internal(
                "merge proposal must contain same-tenant party entities".to_owned(),
            ));
        }

        Ok(ActionEnvelope {
            id: Uuid::new_v4(),
            tenant: survivor.tenant,
            domain: "data".to_owned(),
            action: "merge_parties".to_owned(),
            kind: Some(survivor.kind.clone()),
            targets: proposal.ids.clone(),
            payload: json!({
                "survivor_id": survivor.id,
                "merged_ids": proposal.ids.iter().skip(1).collect::<Vec<_>>(),
                "confidence": proposal.confidence,
                "evidence_count": proposal.evidence.len(),
            }),
            rationale: "DataSteward identity collision proposal".to_owned(),
            reversal: Reversal::Snapshot,
            blast: BlastRadius {
                entities: u32::try_from(proposal.ids.len()).unwrap_or(u32::MAX),
                ..BlastRadius::default()
            },
            invocation: InvocationContext {
                origin_system: Some("hydra".to_owned()),
                external_actor_id: Some("hydra-agent:data-steward".to_owned()),
                external_actor_type: Some("hydra_internal_agent".to_owned()),
                ..InvocationContext::default()
            },
            state: EnvelopeState::Proposed,
            history: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn make_entity(
        id: &str,
        name: Option<&str>,
        email: Option<&str>,
        phone: Option<&str>,
    ) -> Entity {
        let mut body = json!({});
        if let Some(n) = name {
            body.as_object_mut()
                .expect("fixture body should be an object")
                .insert("name".into(), Value::String(n.into()));
        }
        if let Some(e) = email {
            body.as_object_mut()
                .expect("fixture body should be an object")
                .insert("email".into(), Value::String(e.into()));
        }
        if let Some(p) = phone {
            body.as_object_mut()
                .expect("fixture body should be an object")
                .insert("phone".into(), Value::String(p.into()));
        }
        Entity {
            id: Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4()),
            kind: "party".into(),
            tenant: Uuid::nil(),
            body,
            origin: "test".into(),
            origin_ref: None,
            version: 1,
        }
    }

    #[test]
    fn test_deduplicate_empty() {
        let proposals = DataSteward::deduplicate(Uuid::nil(), &[]);
        assert!(proposals.is_empty());
    }

    #[test]
    fn test_deduplicate_single() {
        let e = make_entity(
            "00000000-0000-0000-0000-000000000001",
            Some("Alice"),
            Some("alice@test"),
            None,
        );
        let proposals = DataSteward::deduplicate(Uuid::nil(), &[e]);
        assert!(proposals.is_empty());
    }

    #[test]
    fn test_deduplicate_no_match() {
        let e1 = make_entity(
            "00000000-0000-0000-0000-000000000001",
            Some("Alice"),
            Some("alice@test"),
            None,
        );
        let e2 = make_entity(
            "00000000-0000-0000-0000-000000000002",
            Some("Bob"),
            Some("bob@test"),
            None,
        );
        let proposals = DataSteward::deduplicate(Uuid::nil(), &[e1, e2]);
        assert!(proposals.is_empty());
    }

    #[test]
    fn test_deduplicate_matching_email() {
        let e1 = make_entity(
            "00000000-0000-0000-0000-000000000001",
            Some("Alice Dup"),
            Some("alice@test"),
            None,
        );
        let e2 = make_entity(
            "00000000-0000-0000-0000-000000000002",
            Some("Alice Smith"),
            Some("alice@test"),
            None,
        );
        let proposals = DataSteward::deduplicate(Uuid::nil(), &[e1, e2]);
        assert!(
            !proposals.is_empty(),
            "expected merge proposal for matching email"
        );
        assert_eq!(proposals[0].ids.len(), 2);
        assert!(
            proposals[0]
                .evidence
                .iter()
                .any(|e| e.starts_with("email:")),
            "evidence should include email match"
        );
    }

    #[test]
    fn test_merge_produces_governed_proposal_without_customer_bodies() {
        let tenant = Uuid::new_v4();
        let e1 = Entity {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001")
                .expect("fixture UUID should parse"),
            kind: "party".into(),
            tenant,
            body: json!({"name": "Alice", "email": "alice@test"}),
            origin: "crm1".into(),
            origin_ref: None,
            version: 1,
        };
        let e2 = Entity {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000002")
                .expect("fixture UUID should parse"),
            kind: "party".into(),
            tenant,
            body: json!({"name": "Alice", "phone": "+14255550101"}),
            origin: "crm2".into(),
            origin_ref: None,
            version: 1,
        };
        let proposal = MergeProposal {
            ids: vec![e1.id, e2.id],
            confidence: 1.0,
            evidence: vec!["email:alice@test".into()],
        };

        let envelope = DataSteward::merge(&proposal, &[e1, e2]).expect("merge should propose");
        assert_eq!(envelope.domain, "data");
        assert_eq!(envelope.action, "merge_parties");
        assert_eq!(envelope.state, EnvelopeState::Proposed);
        assert_eq!(envelope.targets, proposal.ids);
        assert_eq!(envelope.payload["evidence_count"], 1);
        assert!(envelope.payload.get("email").is_none());
        assert_eq!(
            envelope.invocation.external_actor_id.as_deref(),
            Some("hydra-agent:data-steward")
        );
    }

    #[test]
    fn test_merge_fails_with_single_id() {
        let proposal = MergeProposal {
            ids: vec![Uuid::parse_str("00000000-0000-0000-0000-000000000001")
                .expect("fixture UUID should parse")],
            confidence: 1.0,
            evidence: vec![],
        };
        let entities = vec![];
        let result = DataSteward::merge(&proposal, &entities);
        assert!(result.is_err());
    }
}
