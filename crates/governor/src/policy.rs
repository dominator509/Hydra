use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{DomainError, Level};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub level: Level,
    pub batch_max: Option<u32>,
}

#[derive(Debug, Default, Clone)]
pub struct PolicyMatrix {
    cells: HashMap<(String, Option<String>, Option<String>), Cell>,
}

impl PolicyMatrix {
    pub fn insert(
        &mut self,
        domain: &str,
        action: Option<&str>,
        kind: Option<&str>,
        cell: Cell,
    ) -> Result<(), DomainError> {
        let key = (
            domain.to_owned(),
            action.map(str::to_owned),
            kind.map(str::to_owned),
        );

        if self.cells.insert(key.clone(), cell).is_some() {
            return Err(DomainError::PolicyResolutionAmbiguous(format!("{key:?}")));
        }

        Ok(())
    }

    pub fn resolve(&self, domain: &str, action: &str, kind: Option<&str>) -> Cell {
        let probes = [
            (
                domain.to_owned(),
                Some(action.to_owned()),
                kind.map(str::to_owned),
            ),
            (domain.to_owned(), Some(action.to_owned()), None),
            (domain.to_owned(), None, None),
        ];

        for probe in probes {
            if let Some(cell) = self.cells.get(&probe) {
                return cell.clone();
            }
        }

        Cell {
            level: Level::L1,
            batch_max: None,
        }
    }
}
