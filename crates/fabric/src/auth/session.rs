use std::str::FromStr;

use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{password, Role, Session};
use crate::FabricError;

pub struct SessionStore {
    pool: PgPool,
}

impl SessionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn authenticate(
        &self,
        username: &str,
        password_str: &str,
    ) -> Result<Session, FabricError> {
        let row = sqlx::query(
            r#"SELECT u.id, u.tenant_id, u.username, u.password_hash
               FROM hydra_user u WHERE u.username = $1"#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(FabricError::AuthzDenied)?;

        let user_id: Uuid = row.get("id");
        let tenant_id: Uuid = row.get("tenant_id");
        let db_username: String = row.get("username");
        let password_hash: String = row.get("password_hash");

        let valid = password::verify_password(password_str, &password_hash)
            .map_err(|_| FabricError::AuthzDenied)?;
        if !valid {
            return Err(FabricError::AuthzDenied);
        }

        let role_rows = sqlx::query(
            r#"SELECT role FROM hydra_role WHERE user_id = $1 AND tenant_id = $2"#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        let roles: Vec<Role> = role_rows
            .into_iter()
            .filter_map(|r| {
                let role_str: String = r.get("role");
                Role::from_str(&role_str).ok()
            })
            .collect();

        let token = Uuid::new_v4().to_string();
        let expires = OffsetDateTime::now_utc() + time::Duration::hours(12);

        sqlx::query(
            r#"INSERT INTO hydra_session (user_id, tenant_id, token, expires_at)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(&token)
        .bind(expires)
        .execute(&self.pool)
        .await?;

        Ok(Session {
            user_id,
            tenant_id,
            username: db_username,
            roles,
            token,
        })
    }

    pub async fn lookup(&self, token: &str) -> Result<Option<Session>, FabricError> {
        let row = sqlx::query(
            r#"SELECT s.user_id, s.tenant_id, u.username, s.token, s.expires_at
               FROM hydra_session s JOIN hydra_user u ON u.id = s.user_id
               WHERE s.token = $1 AND s.expires_at > now()"#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let user_id: Uuid = row.get("user_id");
                let tenant_id: Uuid = row.get("tenant_id");
                let username: String = row.get("username");
                let session_token: String = row.get("token");

                let role_rows = sqlx::query(
                    r#"SELECT role FROM hydra_role WHERE user_id = $1 AND tenant_id = $2"#,
                )
                .bind(user_id)
                .bind(tenant_id)
                .fetch_all(&self.pool)
                .await?;

                let roles: Vec<Role> = role_rows
                    .into_iter()
                    .filter_map(|r| {
                        let role_str: String = r.get("role");
                        Role::from_str(&role_str).ok()
                    })
                    .collect();

                sqlx::query(
                    r#"UPDATE hydra_session SET last_seen_at = now() WHERE token = $1"#,
                )
                .bind(&session_token)
                .execute(&self.pool)
                .await?;

                Ok(Some(Session {
                    user_id,
                    tenant_id,
                    username,
                    roles,
                    token: session_token,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn revoke(&self, token: &str) -> Result<(), FabricError> {
        sqlx::query(r#"DELETE FROM hydra_session WHERE token = $1"#)
            .bind(token)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
