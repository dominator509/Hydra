use sqlx::PgPool;

#[derive(Clone)]
pub struct AdapterKvRepo {
    pool: PgPool,
}

impl AdapterKvRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
