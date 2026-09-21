use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::StoreError;

#[derive(Debug, Clone)]
pub struct AuthUserRecord {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub disabled_at: Option<OffsetDateTime>,
    pub auth_source: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub token: String,
    pub disabled_at: Option<OffsetDateTime>,
    pub auth_source: String,
    pub roles: Vec<String>,
}

#[derive(Clone)]
pub struct SessionRepo {
    pool: PgPool,
}

impl SessionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn find_user_for_auth(
        &self,
        username: &str,
    ) -> Result<Option<AuthUserRecord>, StoreError> {
        let row = sqlx::query(
            r#"SELECT u.id, u.tenant_id, u.username, u.password_hash,
                      u.disabled_at, u.auth_source
               FROM hydra_user u WHERE u.username = $1"#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let user_id: Uuid = row.get("id");
        let tenant_id: Uuid = row.get("tenant_id");
        Ok(Some(AuthUserRecord {
            user_id,
            tenant_id,
            username: row.get("username"),
            password_hash: row.get("password_hash"),
            disabled_at: row.get("disabled_at"),
            auth_source: row.get("auth_source"),
            roles: self.roles(user_id, tenant_id).await?,
        }))
    }

    pub async fn create_session(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
        token: &str,
        expires_at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let token_hash = token_hash(token);
        sqlx::query(
            r#"INSERT INTO hydra_session (user_id, tenant_id, token_hash, expires_at)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_active_session(
        &self,
        token: &str,
    ) -> Result<Option<SessionRecord>, StoreError> {
        let token_hash = token_hash(token);
        let row = sqlx::query(
            r#"SELECT s.id, s.user_id, s.tenant_id, u.username, s.token_hash,
                      u.disabled_at, u.auth_source
               FROM hydra_session s JOIN hydra_user u ON u.id = s.user_id
               WHERE (s.token_hash = $1 OR s.token = $2)
                 AND s.expires_at > now()"#,
        )
        .bind(&token_hash)
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let session_id: Uuid = row.get("id");
        let user_id: Uuid = row.get("user_id");
        let tenant_id: Uuid = row.get("tenant_id");
        let stored_hash: Option<String> = row.get("token_hash");
        if stored_hash.is_none() {
            // Upgrade a legacy row only after the caller proves possession.
            // A concurrent revoke or upgrader may win the zero-row race.
            let _ = sqlx::query(
                r#"UPDATE hydra_session
                   SET token_hash = $1, token = NULL
                   WHERE id = $2 AND token_hash IS NULL AND token = $3"#,
            )
            .bind(&token_hash)
            .bind(session_id)
            .bind(token)
            .execute(&self.pool)
            .await?;
        }
        Ok(Some(SessionRecord {
            user_id,
            tenant_id,
            username: row.get("username"),
            // Return the presented bearer token to the auth layer; it is not
            // read back from durable storage after the hash migration.
            token: token.to_owned(),
            disabled_at: row.get("disabled_at"),
            auth_source: row.get("auth_source"),
            roles: self.roles(user_id, tenant_id).await?,
        }))
    }

    pub async fn touch(&self, token: &str) -> Result<(), StoreError> {
        let token_hash = token_hash(token);
        sqlx::query(
            r#"UPDATE hydra_session
               SET last_seen_at = now()
               WHERE token_hash = $1 OR token = $2"#,
        )
        .bind(token_hash)
        .bind(token)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn revoke(&self, token: &str) -> Result<(), StoreError> {
        let token_hash = token_hash(token);
        sqlx::query(
            r#"DELETE FROM hydra_session
               WHERE token_hash = $1 OR token = $2"#,
        )
        .bind(token_hash)
        .bind(token)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn roles(&self, user_id: Uuid, tenant_id: Uuid) -> Result<Vec<String>, StoreError> {
        let rows =
            sqlx::query(r#"SELECT role FROM hydra_role WHERE user_id = $1 AND tenant_id = $2"#)
                .bind(user_id)
                .bind(tenant_id)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|row| row.get("role")).collect())
    }
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

impl From<PgPool> for SessionRepo {
    fn from(pool: PgPool) -> Self {
        Self::new(pool)
    }
}
