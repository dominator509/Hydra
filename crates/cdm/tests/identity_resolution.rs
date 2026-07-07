use cdm::{proposals, MergeProposal, PartyView};
use uuid::Uuid;

#[test]
fn identity_deterministic_email_keys_are_case_insensitive() {
    let party_a = party("Ada Lovelace", Some("Ada@Example.com"), None, None);
    let party_b = party("ADA LOVELACE", Some("ada@example.com"), None, None);

    let proposals = proposals(&[party_a.clone(), party_b.clone()]);

    assert_eq!(
        proposals,
        vec![MergeProposal {
            ids: sorted_ids(vec![party_a.id, party_b.id]),
            confidence: 1.0,
            evidence: vec!["email:ada@example.com".to_owned()],
        }]
    );
}

#[test]
fn identity_deterministic_phone_keys_normalize_to_e164_style() {
    let party_a = party("Ada", None, Some("(555) 123-4567"), None);
    let party_b = party("Ada", None, Some("+1 555 123 4567"), None);

    let proposals = proposals(&[party_a.clone(), party_b.clone()]);

    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].confidence, 1.0);
    assert_eq!(proposals[0].evidence, vec!["phone:+15551234567".to_owned()]);
}

#[test]
fn identity_deterministic_domain_keys_normalize_host_variants() {
    let party_a = party("Acme", None, None, Some("https://www.example.com/path"));
    let party_b = party("Acme", None, None, Some("example.com"));

    let proposals = proposals(&[party_a.clone(), party_b.clone()]);

    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].confidence, 1.0);
    assert_eq!(proposals[0].evidence, vec!["domain:example.com".to_owned()]);
}

#[test]
fn identity_fuzzy_name_candidates_emit_sub_unit_confidence() {
    let party_a = party("Ada Lovelace", None, None, None);
    let party_b = party("Ada Lovelace", None, None, None);

    let proposals = proposals(&[party_a.clone(), party_b.clone()]);

    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].confidence, 0.6);
    assert_eq!(proposals[0].evidence, vec!["name:ada lovelace".to_owned()]);
}

fn party(
    display_name: &str,
    email: Option<&str>,
    phone: Option<&str>,
    domain: Option<&str>,
) -> PartyView {
    PartyView {
        id: Uuid::new_v4(),
        display_name: Some(display_name.to_owned()),
        email: email.map(str::to_owned),
        phone: phone.map(str::to_owned),
        domain: domain.map(str::to_owned),
    }
}

fn sorted_ids(mut ids: Vec<Uuid>) -> Vec<Uuid> {
    ids.sort_unstable();
    ids
}
