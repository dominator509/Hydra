//! DataSteward — identity deduplication and merge orchestration.
//!
//! Operates on `cdm::Entity` values, proposes merges based on collision
//! heuristics, and executes merges by consolidating bodies.

use cdm::{proposals, Entity, MergeProposal};
use uuid::Uuid;

use crate::bridge_engineer::AgentError;

/// A deduplication context tied to a tenant.
///
/// The DataSteward holds no mutable state itself — all state is passed
/// through method arguments, making it testable without fixtures.
pub struct DataSteward;

impl DataSteward {
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
                display_name: e.body.get("name").and_then(|v| v.as_str().map(|s| s.to_owned())),
                email: e.body.get("email").and_then(|v| v.as_str().map(|s| s.to_owned())),
                phone: e.body.get("phone").and_then(|v| v.as_str().map(|s| s.to_owned())),
                domain: e.body.get("domain").and_then(|v| v.as_str().map(|s| s.to_owned())),
            })
            .collect();

        proposals(&party_views)
    }

    /// Execute a merge proposal, returning the surviving consolidated entity.
    ///
    /// The merge consolidates all JSON object bodies from the constituent
    /// entities into a single body. Non-object bodies are skipped.
    /// The first entity's id is used as the survivor.
    pub fn merge(proposal: &MergeProposal, entities: &[Entity]) -> Result<Entity, AgentError> {
        if proposal.ids.len() < 2 {
            return Err(AgentError::Internal(
                "merge requires at least 2 entity ids".into(),
            ));
        }

        let id_set: std::collections::HashSet<Uuid> = proposal.ids.iter().cloned().collect();
        let involved: Vec<&Entity> = entities
            .iter()
            .filter(|e| id_set.contains(&e.id))
            .collect();

        if involved.len() < 2 {
            return Err(AgentError::Internal(
                "merge proposal ids not found in entity slice".into(),
            ));
        }

        let survivor = involved[0];
        let mut merged_body = survivor.body.clone();

        for entity in &involved[1..] {
            if let Some(obj) = entity.body.as_object() {
                if let Some(survivor_obj) = merged_body.as_object_mut() {
                    for (key, val) in obj {
                        // Only fill missing fields; do not overwrite.
                        survivor_obj.entry(key.as_str()).or_insert_with(|| val.clone());
                    }
                }
            }
        }

        Ok(Entity {
            id: survivor.id,
            kind: survivor.kind.clone(),
            tenant: survivor.tenant,
            body: merged_body,
            origin: survivor.origin.clone(),
            origin_ref: survivor.origin_ref.clone(),
            version: survivor.version + 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn make_entity(id: &str, name: Option<&str>, email: Option<&str>, phone: Option<&str>) -> Entity {
        let mut body = json!({});
        if let Some(n) = name {
            body.as_object_mut().unwrap().insert("name".into(), Value::String(n.into()));
        }
        if let Some(e) = email {
            body.as_object_mut().unwrap().insert("email".into(), Value::String(e.into()));
        }
        if let Some(p) = phone {
            body.as_object_mut().unwrap().insert("phone".into(), Value::String(p.into()));
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
        let e = make_entity("00000000-0000-0000-0000-000000000001", Some("Alice"), Some("alice@test"), None);
        let proposals = DataSteward::deduplicate(Uuid::nil(), &[e]);
        assert!(proposals.is_empty());
    }

    #[test]
    fn test_deduplicate_no_match() {
        let e1 = make_entity("00000000-0000-0000-0000-000000000001", Some("Alice"), Some("alice@test"), None);
        let e2 = make_entity("00000000-0000-0000-0000-000000000002", Some("Bob"), Some("bob@test"), None);
        let proposals = DataSteward::deduplicate(Uuid::nil(), &[e1, e2]);
        assert!(proposals.is_empty());
    }

    #[test]
    fn test_deduplicate_matching_email() {
        let e1 = make_entity("00000000-0000-0000-0000-000000000001", Some("Alice Dup"), Some("alice@test"), None);
        let e2 = make_entity("00000000-0000-0000-0000-000000000002", Some("Alice Smith"), Some("alice@test"), None);
        let proposals = DataSteward::deduplicate(Uuid::nil(), &[e1, e2]);
        assert!(!proposals.is_empty(), "expected merge proposal for matching email");
        assert_eq!(proposals[0].ids.len(), 2);
        assert!(
            proposals[0].evidence.iter().any(|e| e.starts_with("email:")),
            "evidence should include email match"
        );
    }

    #[test]
    fn test_merge_consolidates_bodies() {
        let tenant = Uuid::nil();
        let e1 = Entity {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            kind: "party".into(),
            tenant,
            body: json!({"name": "Alice", "email": "alice@test"}),
            origin: "crm1".into(),
            origin_ref: None,
            version: 1,
        };
        let e2 = Entity {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
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

        let merged = DataSteward::merge(&proposal, &[e1, e2]).unwrap();
        assert_eq!(merged.id, Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());
        assert_eq!(merged.body["name"], "Alice");
        assert_eq!(merged.body["email"], "alice@test");
        assert_eq!(merged.body["phone"], "+14255550101");
        assert_eq!(merged.version, 2);
    }

    #[test]
    fn test_merge_fails_with_single_id() {
        let proposal = MergeProposal {
            ids: vec![Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()],
            confidence: 1.0,
            evidence: vec![],
        };
        let entities = vec![];
        let result = DataSteward::merge(&proposal, &entities);
        assert!(result.is_err());
    }
}
