use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::StoreError;

const MAX_USERNAME_BYTES: usize = 128;
const MAX_DISPLAY_NAME_BYTES: usize = 200;
const MAX_PASSWORD_HASH_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorUser {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub role: String,
    pub disabled_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOperatorUser {
    pub tenant_id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub role: String,
}

impl NewOperatorUser {
    fn validate(&self) -> Result<(), StoreError> {
        if self.tenant_id.is_nil() {
            return Err(StoreError::Invariant(
                "operator tenant_id cannot be nil".to_owned(),
            ));
        }
        validate_bounded_text("username", &self.username, MAX_USERNAME_BYTES)?;
        if self.username.trim() != self.username
            || self
                .username
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte == b' ')
        {
            return Err(StoreError::Invariant(
                "operator username contains invalid whitespace".to_owned(),
            ));
        }
        if self.password_hash.len() > MAX_PASSWORD_HASH_BYTES
            || !self.password_hash.starts_with("$argon2id$")
        {
            return Err(StoreError::Invariant(
                "operator password_hash must be an Argon2id encoding".to_owned(),
            ));
        }
        if let Some(display_name) = &self.display_name {
            validate_bounded_text("display_name", display_name, MAX_DISPLAY_NAME_BYTES)?;
        }
        validate_role(&self.role)
    }
}

struct OperatorRow {
    id: Uuid,
    tenant_id: Uuid,
    username: String,
    display_name: Option<String>,
    role: String,
    disabled_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

struct OperatorIdRow {
    id: Uuid,
}

#[derive(Clone)]
pub struct OperatorRepo {
    pool: PgPool,
}

impl OperatorRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, user: NewOperatorUser) -> Result<OperatorUser, StoreError> {
        user.validate()?;
        let mut tx = self.pool.begin().await?;
        let user_id = sqlx::query_as!(
            OperatorIdRow,
            r#"
            INSERT INTO hydra_user (
                tenant_id,
                username,
                password_hash,
                display_name,
                auth_source
            )
            VALUES ($1, $2, $3, $4, 'operator')
            RETURNING id
            "#,
            user.tenant_id,
            user.username,
            user.password_hash,
            user.display_name,
        )
        .fetch_one(&mut *tx)
        .await?
        .id;

        sqlx::query!(
            r#"
            INSERT INTO hydra_role (user_id, tenant_id, role)
            VALUES ($1, $2, $3)
            "#,
            user_id,
            user.tenant_id,
            user.role,
        )
        .execute(&mut *tx)
        .await?;

        let row = Self::get_row(&mut *tx, user.tenant_id, user_id).await?;
        tx.commit().await?;
        row.map(row_to_user)
            .transpose()?
            .ok_or(StoreError::NotFound)
    }

    pub async fn list_for_tenant(&self, tenant_id: Uuid) -> Result<Vec<OperatorUser>, StoreError> {
        if tenant_id.is_nil() {
            return Err(StoreError::Invariant(
                "operator tenant_id cannot be nil".to_owned(),
            ));
        }
        let rows = sqlx::query_as!(
            OperatorRow,
            r#"
            SELECT u.id, u.tenant_id, u.username, u.display_name, r.role,
                   u.disabled_at, u.created_at, u.updated_at
            FROM hydra_user u
            JOIN hydra_role r ON r.user_id = u.id AND r.tenant_id = u.tenant_id
            WHERE u.tenant_id = $1
              AND u.auth_source = 'operator'
            ORDER BY u.username, u.id
            "#,
            tenant_id
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_user).collect()
    }

    pub async fn set_disabled(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        disabled: bool,
    ) -> Result<OperatorUser, StoreError> {
        if tenant_id.is_nil() || user_id.is_nil() {
            return Err(StoreError::Invariant(
                "operator tenant_id and user_id cannot be nil".to_owned(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query!(
            r#"
            UPDATE hydra_user
            SET disabled_at = CASE WHEN $3 THEN COALESCE(disabled_at, now()) ELSE NULL END,
                updated_at = now()
            WHERE id = $1
              AND tenant_id = $2
              AND auth_source = 'operator'
            "#,
            user_id,
            tenant_id,
            disabled,
        )
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::NotFound);
        }
        if disabled {
            sqlx::query!(
                "DELETE FROM hydra_session WHERE user_id = $1 AND tenant_id = $2",
                user_id,
                tenant_id,
            )
            .execute(&mut *tx)
            .await?;
        }
        let row = Self::get_row(&mut *tx, tenant_id, user_id).await?;
        tx.commit().await?;
        row.map(row_to_user)
            .transpose()?
            .ok_or(StoreError::NotFound)
    }

    async fn get_row<'a, E>(
        executor: E,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<OperatorRow>, sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Postgres>,
    {
        sqlx::query_as!(
            OperatorRow,
            r#"
            SELECT u.id, u.tenant_id, u.username, u.display_name, r.role,
                   u.disabled_at, u.created_at, u.updated_at
            FROM hydra_user u
            JOIN hydra_role r ON r.user_id = u.id AND r.tenant_id = u.tenant_id
            WHERE u.id = $1
              AND u.tenant_id = $2
              AND u.auth_source = 'operator'
            "#,
            user_id,
            tenant_id
        )
        .fetch_optional(executor)
        .await
    }
}

pub fn validate_role(role: &str) -> Result<(), StoreError> {
    if matches!(role, "viewer" | "operator" | "approver" | "admin") {
        Ok(())
    } else {
        Err(StoreError::Invariant(format!(
            "unsupported operator role '{role}'"
        )))
    }
}

fn validate_bounded_text(name: &str, value: &str, max_bytes: usize) -> Result<(), StoreError> {
    if value.trim().is_empty() || value.len() > max_bytes {
        return Err(StoreError::Invariant(format!(
            "operator {name} must be non-empty and at most {max_bytes} bytes"
        )));
    }
    Ok(())
}

fn row_to_user(row: OperatorRow) -> Result<OperatorUser, StoreError> {
    validate_role(&row.role)?;
    Ok(OperatorUser {
        id: row.id,
        tenant_id: row.tenant_id,
        username: row.username,
        display_name: row.display_name,
        role: row.role,
        disabled_at: row.disabled_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
