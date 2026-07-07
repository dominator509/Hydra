use std::env;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use uuid::Uuid;

use crate::StoreError;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

pub struct TestDb {
    pub admin_pool: PgPool,
    pub pool: PgPool,
    pub schema: String,
}

impl TestDb {
    pub async fn new() -> Result<Self, StoreError> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|error| StoreError::Invariant(format!("DATABASE_URL missing: {error}")))?;
        let admin_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await?;

        let schema = format!("hydra_test_{}", Uuid::new_v4().simple());
        let create_schema = format!("CREATE SCHEMA {schema}");
        sqlx::query(&create_schema).execute(&admin_pool).await?;

        let connect_options: PgConnectOptions = database_url
            .parse()
            .map_err(|error| StoreError::Invariant(format!("invalid DATABASE_URL: {error}")))?;
        let search_path = format!("{schema},public");
        let connect_options = connect_options.options([("search_path", search_path.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(connect_options)
            .await?;
        MIGRATOR.run(&pool).await?;

        Ok(Self {
            admin_pool,
            pool,
            schema,
        })
    }

    pub async fn cleanup(self) -> Result<(), StoreError> {
        self.pool.close().await;
        let drop_schema = format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema);
        sqlx::query(&drop_schema).execute(&self.admin_pool).await?;
        self.admin_pool.close().await;
        Ok(())
    }
}
