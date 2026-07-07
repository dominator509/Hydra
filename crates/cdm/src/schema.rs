use std::collections::BTreeMap;

use jsonschema::{Draft, JSONSchema};
use serde_json::Value;

use crate::kinds::builtin_kind_schemas;
use crate::DomainError;

struct KindSchema {
    raw: Value,
    validator: JSONSchema,
}

pub struct KindRegistry {
    schemas: BTreeMap<String, KindSchema>,
}

impl Default for KindRegistry {
    fn default() -> Self {
        let mut registry = Self {
            schemas: BTreeMap::new(),
        };

        for (kind, schema) in builtin_kind_schemas() {
            registry
                .register(&kind, schema)
                .expect("builtin kind schema must compile");
        }

        registry
    }
}

impl KindRegistry {
    pub fn register(&mut self, kind: &str, schema: Value) -> Result<(), DomainError> {
        let validator = JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(&schema)
            .map_err(|error| DomainError::InvalidSchema {
                kind: kind.to_owned(),
                message: error.to_string(),
            })?;

        self.schemas.insert(
            kind.to_owned(),
            KindSchema {
                raw: schema,
                validator,
            },
        );

        Ok(())
    }

    pub fn validate(&self, kind: &str, body: &Value) -> Result<(), DomainError> {
        let kind_schema = self
            .schemas
            .get(kind)
            .ok_or_else(|| DomainError::UnknownKind(kind.to_owned()))?;

        if let Err(mut errors) = kind_schema.validator.validate(body) {
            let error = errors
                .next()
                .expect("validation errors iterator must contain an item");
            let path = if error.instance_path.to_string().is_empty() {
                "$".to_owned()
            } else {
                format!("${}", error.instance_path)
            };
            return Err(DomainError::SchemaViolation {
                path,
                message: error.to_string(),
            });
        }

        Ok(())
    }

    pub fn schema(&self, kind: &str) -> Option<&Value> {
        self.schemas.get(kind).map(|schema| &schema.raw)
    }
}
