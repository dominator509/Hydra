use cdm::{builtin_kind_names, DomainError, KindRegistry};
use serde_json::json;

#[test]
fn regress_schema_validation_accepts_each_builtin_fixture() {
    let registry = KindRegistry::default();

    let fixtures = [
        (
            "party",
            json!({ "display_name": "Ada Lovelace", "email": "ada@example.com" }),
        ),
        ("deal", json!({ "title": "Renewal", "amount_cents": 1000 })),
        ("pipeline", json!({ "name": "Default" })),
        (
            "stage",
            json!({ "name": "Qualified", "pipeline_id": "pipeline-1" }),
        ),
        (
            "activity",
            json!({ "activity_type": "call", "occurred_at": "2026-01-01T00:00:00Z" }),
        ),
        ("ticket", json!({ "subject": "Support request" })),
        (
            "campaign",
            json!({ "name": "Spring launch", "budget_cents": 5000 }),
        ),
        ("asset", json!({ "name": "Deck", "asset_type": "document" })),
    ];

    assert_eq!(builtin_kind_names().len(), fixtures.len());

    for (kind, body) in fixtures {
        registry
            .validate(kind, &body)
            .unwrap_or_else(|error| panic!("fixture for kind '{kind}' should validate: {error}"));
    }
}

#[test]
fn regress_schema_validation_reports_unknown_kind() {
    let registry = KindRegistry::default();

    let error = registry.validate("unknown", &json!({ "display_name": "Ada" }));

    assert!(matches!(error, Err(DomainError::UnknownKind(kind)) if kind == "unknown"));
}

#[test]
fn regress_schema_validation_reports_path_and_message() {
    let registry = KindRegistry::default();

    let error = registry
        .validate("deal", &json!({ "title": 42 }))
        .expect_err("deal.title as an integer should fail schema validation");

    match error {
        DomainError::SchemaViolation { path, message } => {
            assert_eq!(path, "$/title");
            assert!(message.contains("string"));
        }
        other => panic!("expected schema violation, got {other:?}"),
    }
}
