use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use governor::{Constitution, Governor};
use tokio::sync::RwLock;
use uuid::Uuid;

struct CachedGovernor {
    revision: u64,
    governor: Arc<Governor>,
}

#[derive(Clone)]
pub struct PersistedGovernorProvider {
    autonomy: store::AutonomyRepo,
    constitution: Constitution,
    cache: Arc<RwLock<BTreeMap<Uuid, CachedGovernor>>>,
}

impl PersistedGovernorProvider {
    pub fn new(autonomy: store::AutonomyRepo, constitution: Constitution) -> Self {
        Self {
            autonomy,
            constitution,
            cache: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub async fn cached_tenants(&self) -> usize {
        self.cache.read().await.len()
    }
}

#[async_trait]
impl fabric::GovernorProvider for PersistedGovernorProvider {
    async fn governor(&self, tenant: Uuid) -> Result<Arc<Governor>, fabric::FabricError> {
        if tenant.is_nil() {
            return Err(fabric::FabricError::AuthzDenied);
        }
        let revision = self.autonomy.revision(tenant).await?;
        if let Some(cached) = self.cache.read().await.get(&tenant) {
            if cached.revision == revision {
                return Ok(cached.governor.clone());
            }
        }

        let governor = Arc::new(Governor {
            matrix: self.autonomy.matrix(tenant).await?,
            constitution: self.constitution.clone(),
        });
        self.cache.write().await.insert(
            tenant,
            CachedGovernor {
                revision,
                governor: governor.clone(),
            },
        );
        Ok(governor)
    }
}
