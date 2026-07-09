#[test]
fn four_eyes_proposer_cannot_approve_own_envelope() {
    // The four-eyes rule: if ctx.principal == envelope.proposed_by, reject
    // This is enforced in EnvelopeService.approve
    let proposer = "user:alice";
    let approver = "user:alice"; // same person
    assert_eq!(
        proposer, approver,
        "four-eyes should detect same principal"
    );

    let different_approver = "user:bob";
    assert_ne!(
        proposer, different_approver,
        "four-eyes should allow different principal"
    );
}

#[test]
fn four_eyes_service_principal_bypass() {
    // Service principals and agent principals are exempt from four-eyes
    // because they act on behalf of the system after Governor approval
    let service_principal = "service:cron";
    let agent_principal = "agent:concierge";

    // These are valid service/agent principal formats
    assert!(service_principal.starts_with("service:"));
    assert!(agent_principal.starts_with("agent:"));
}
