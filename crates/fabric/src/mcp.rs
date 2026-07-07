use serde_json::{json, Value};

pub fn tool_schema() -> Value {
    json!({
        "protocol": "mcp",
        "version": "1.0.0",
        "tools": [
            {
                "name": "hydra.search_entities",
                "inputSchema": {
                    "type": "object",
                    "required": ["kind", "query"],
                    "properties": {
                        "kind": { "type": "string" },
                        "query": { "type": "string" }
                    }
                }
            },
            {
                "name": "hydra.get_entity",
                "inputSchema": {
                    "type": "object",
                    "required": ["kind", "id"],
                    "properties": {
                        "kind": { "type": "string" },
                        "id": { "type": "string", "format": "uuid" }
                    }
                }
            },
            {
                "name": "hydra.propose_envelope",
                "inputSchema": {
                    "type": "object",
                    "required": ["domain", "action", "targets", "payload", "rationale"],
                    "properties": {
                        "domain": { "type": "string" },
                        "action": { "type": "string" },
                        "targets": {
                            "type": "array",
                            "items": { "type": "string", "format": "uuid" }
                        },
                        "payload": { "type": "object" },
                        "rationale": { "type": "string" }
                    }
                }
            },
            {
                "name": "hydra.list_pending",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "hydra.approve",
                "inputSchema": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {
                        "id": { "type": "string", "format": "uuid" }
                    }
                }
            },
            {
                "name": "hydra.pipeline_stats",
                "inputSchema": {
                    "type": "object",
                    "required": ["pipeline_id"],
                    "properties": {
                        "pipeline_id": { "type": "string", "format": "uuid" }
                    }
                }
            },
            {
                "name": "hydra.tk_stats",
                "inputSchema": {
                    "type": "object",
                    "required": ["window"],
                    "properties": {
                        "window": { "type": "string" }
                    }
                }
            }
        ]
    })
}
