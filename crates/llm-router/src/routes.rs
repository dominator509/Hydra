use std::collections::HashMap;

use serde::Deserialize;

pub type Routes = HashMap<String, RouteCfg>;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RouteCfg {
    pub name: String,
    pub pii: bool,
    pub max_tokens: u32,
    pub output_budget_bytes: usize,
    pub providers: Vec<String>,
    #[serde(default)]
    pub tk_exempt: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RoutesError {
    #[error("invalid routes yaml: {0}")]
    Yaml(String),
    #[error("duplicate llm route '{0}'")]
    DuplicateRoute(String),
    #[error("route '{0}' has no providers")]
    EmptyProviders(String),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RoutesDocument {
    List(Vec<RouteCfg>),
    Wrapped { routes: Vec<RouteCfg> },
}

pub fn load_routes_yaml(input: &str) -> Result<Routes, RoutesError> {
    let document: RoutesDocument =
        serde_yaml::from_str(input).map_err(|error| RoutesError::Yaml(error.to_string()))?;
    let items = match document {
        RoutesDocument::List(routes) => routes,
        RoutesDocument::Wrapped { routes } => routes,
    };

    let mut parsed = HashMap::with_capacity(items.len());
    for route in items {
        if route.providers.is_empty() {
            return Err(RoutesError::EmptyProviders(route.name));
        }
        let route_name = route.name.clone();
        if parsed.insert(route.name.clone(), route).is_some() {
            return Err(RoutesError::DuplicateRoute(route_name));
        }
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_yaml_loader_parses_wrapped_documents() {
        let routes = load_routes_yaml(
            r#"
routes:
  - name: concierge
    pii: false
    max_tokens: 256
    output_budget_bytes: 2048
    providers: [deepseek, local]
  - name: comms
    pii: true
    max_tokens: 128
    output_budget_bytes: 1024
    providers: [private]
    tk_exempt: true
"#,
        )
        .expect("wrapped routes yaml should parse");

        assert_eq!(routes.len(), 2);
        assert_eq!(routes["concierge"].providers, vec!["deepseek", "local"]);
        assert!(routes["comms"].tk_exempt);
    }
}
