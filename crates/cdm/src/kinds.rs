use std::collections::BTreeMap;

use serde_json::{json, Value};

pub fn builtin_kind_names() -> Vec<&'static str> {
    vec![
        "party", "deal", "pipeline", "stage", "activity", "ticket", "campaign", "asset",
    ]
}

pub fn builtin_kind_schemas() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "party".to_owned(),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "required": ["display_name"],
                "properties": {
                    "display_name": { "type": "string", "minLength": 1 },
                    "party_type": { "type": "string" },
                    "email": { "type": "string" },
                    "phone": { "type": "string" },
                    "domain": { "type": "string" }
                },
                "additionalProperties": true
            }),
        ),
        (
            "deal".to_owned(),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "required": ["title"],
                "properties": {
                    "title": { "type": "string", "minLength": 1 },
                    "amount_cents": { "type": "integer", "minimum": 0 },
                    "stage_id": { "type": "string" },
                    "pipeline_id": { "type": "string" }
                },
                "additionalProperties": true
            }),
        ),
        (
            "pipeline".to_owned(),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "is_default": { "type": "boolean" }
                },
                "additionalProperties": true
            }),
        ),
        (
            "stage".to_owned(),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "required": ["name", "pipeline_id"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "pipeline_id": { "type": "string", "minLength": 1 },
                    "position": { "type": "integer", "minimum": 0 }
                },
                "additionalProperties": true
            }),
        ),
        (
            "activity".to_owned(),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "required": ["activity_type", "occurred_at"],
                "properties": {
                    "activity_type": { "type": "string", "minLength": 1 },
                    "occurred_at": { "type": "string", "minLength": 1 },
                    "summary": { "type": "string" }
                },
                "additionalProperties": true
            }),
        ),
        (
            "ticket".to_owned(),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "required": ["subject"],
                "properties": {
                    "subject": { "type": "string", "minLength": 1 },
                    "status": { "type": "string" },
                    "priority": { "type": "string" }
                },
                "additionalProperties": true
            }),
        ),
        (
            "campaign".to_owned(),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "channel": { "type": "string" },
                    "budget_cents": { "type": "integer", "minimum": 0 }
                },
                "additionalProperties": true
            }),
        ),
        (
            "asset".to_owned(),
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "required": ["name", "asset_type"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "asset_type": { "type": "string", "minLength": 1 },
                    "uri": { "type": "string" }
                },
                "additionalProperties": true
            }),
        ),
    ])
}
