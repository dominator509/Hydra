use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyView {
    pub id: Uuid,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergeProposal {
    pub ids: Vec<Uuid>,
    pub confidence: f32,
    pub evidence: Vec<String>,
}

#[derive(Debug, Default)]
struct ProposalBuilder {
    confidence: f32,
    evidence: BTreeSet<String>,
}

pub fn proposals(parties: &[PartyView]) -> Vec<MergeProposal> {
    let mut grouped: BTreeMap<Vec<Uuid>, ProposalBuilder> = BTreeMap::new();
    let mut deterministic_pairs = BTreeSet::new();

    let mut deterministic_groups: BTreeMap<String, Vec<Uuid>> = BTreeMap::new();
    for party in parties {
        if let Some(key) = canonical_email(party.email.as_deref()) {
            deterministic_groups
                .entry(format!("email:{key}"))
                .or_default()
                .push(party.id);
        }
        if let Some(key) = canonical_phone(party.phone.as_deref()) {
            deterministic_groups
                .entry(format!("phone:{key}"))
                .or_default()
                .push(party.id);
        }
        if let Some(key) = canonical_domain(party.domain.as_deref()) {
            deterministic_groups
                .entry(format!("domain:{key}"))
                .or_default()
                .push(party.id);
        }
    }

    for (evidence, ids) in deterministic_groups {
        if ids.len() < 2 {
            continue;
        }
        record_deterministic_pairs(&mut deterministic_pairs, &ids);
        merge_group(&mut grouped, ids, 1.0, evidence);
    }

    for (left_index, left) in parties.iter().enumerate() {
        let Some(left_name) = canonical_name(left.display_name.as_deref()) else {
            continue;
        };

        for right in parties.iter().skip(left_index + 1) {
            let pair = sorted_pair(left.id, right.id);
            if deterministic_pairs.contains(&pair) {
                continue;
            }

            let Some(right_name) = canonical_name(right.display_name.as_deref()) else {
                continue;
            };
            if left_name != right_name {
                continue;
            }

            let confidence = match (
                canonical_domain(left.domain.as_deref()),
                canonical_domain(right.domain.as_deref()),
            ) {
                (Some(left_domain), Some(right_domain)) if left_domain == right_domain => 0.8,
                _ => 0.6,
            };

            merge_group(
                &mut grouped,
                vec![left.id, right.id],
                confidence,
                format!("name:{left_name}"),
            );
        }
    }

    let mut proposals = grouped
        .into_iter()
        .map(|(ids, builder)| MergeProposal {
            ids,
            confidence: builder.confidence,
            evidence: builder.evidence.into_iter().collect(),
        })
        .collect::<Vec<_>>();

    proposals.sort_by(|left, right| {
        right
            .confidence
            .partial_cmp(&left.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.ids.cmp(&right.ids))
    });

    proposals
}

fn merge_group(
    grouped: &mut BTreeMap<Vec<Uuid>, ProposalBuilder>,
    mut ids: Vec<Uuid>,
    confidence: f32,
    evidence: String,
) {
    ids.sort_unstable();
    ids.dedup();
    if ids.len() < 2 {
        return;
    }

    let builder = grouped.entry(ids).or_default();
    builder.confidence = builder.confidence.max(confidence);
    builder.evidence.insert(evidence);
}

fn canonical_email(email: Option<&str>) -> Option<String> {
    let email = email?.trim().to_ascii_lowercase();
    if email.is_empty() {
        return None;
    }
    Some(email)
}

fn canonical_phone(phone: Option<&str>) -> Option<String> {
    let phone = phone?.trim();
    if phone.is_empty() {
        return None;
    }

    let digits = phone
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>();
    if !(8..=15).contains(&digits.len()) {
        return None;
    }

    if digits.len() == 10 {
        return Some(format!("+1{digits}"));
    }
    if digits.len() == 11 && digits.starts_with('1') {
        return Some(format!("+{digits}"));
    }

    Some(format!("+{digits}"))
}

fn canonical_domain(domain: Option<&str>) -> Option<String> {
    let mut domain = domain?.trim().to_ascii_lowercase();
    if domain.is_empty() {
        return None;
    }

    if let Some(rest) = domain.strip_prefix("https://") {
        domain = rest.to_owned();
    } else if let Some(rest) = domain.strip_prefix("http://") {
        domain = rest.to_owned();
    }

    domain = domain
        .split('/')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_owned();

    if let Some(rest) = domain.strip_prefix("www.") {
        domain = rest.to_owned();
    }

    if domain.is_empty() {
        return None;
    }

    Some(domain)
}

fn canonical_name(name: Option<&str>) -> Option<String> {
    let name = name?
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>();
    let normalized = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    Some(normalized)
}

fn record_deterministic_pairs(pairs: &mut BTreeSet<Vec<Uuid>>, ids: &[Uuid]) {
    let mut sorted = ids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    for left_index in 0..sorted.len() {
        for right_index in left_index + 1..sorted.len() {
            pairs.insert(vec![sorted[left_index], sorted[right_index]]);
        }
    }
}

fn sorted_pair(left: Uuid, right: Uuid) -> Vec<Uuid> {
    let mut pair = vec![left, right];
    pair.sort_unstable();
    pair
}
