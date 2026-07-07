use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub adapter_id: String,
    pub origins: Vec<String>,
    pub secret_names: Vec<String>,
    pub dsn_name: Option<String>,
    pub fuel: u64,
}

impl Grant {
    pub fn origin_allowed(&self, url: &str) -> bool {
        self.origins.iter().any(|origin| {
            url.starts_with(origin)
                && url[origin.len()..]
                    .chars()
                    .next()
                    .map_or(true, |ch| ch == '/' || ch == '?')
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct GrantTable {
    grants: HashMap<String, Grant>,
}

impl GrantTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, grant: Grant) -> Option<Grant> {
        self.grants.insert(grant.adapter_id.clone(), grant)
    }

    pub fn get(&self, adapter_id: &str) -> Option<&Grant> {
        self.grants.get(adapter_id)
    }
}

impl FromIterator<Grant> for GrantTable {
    fn from_iter<T: IntoIterator<Item = Grant>>(iter: T) -> Self {
        let mut table = Self::new();
        for grant in iter {
            table.insert(grant);
        }
        table
    }
}
