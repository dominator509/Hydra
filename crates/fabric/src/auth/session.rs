use std::str::FromStr;

use time::OffsetDateTime;
use uuid::Uuid;

use super::{password, Role, Session};
use crate::FabricError;

pub struct SessionStore {
    repo: store::SessionRepo,
}

impl SessionStore {
    pub fn new(repo: impl Into<store::SessionRepo>) -> Self {
        Self { repo: repo.into() }
    }

    pub async fn authenticate(
        &self,
        username: &str,
        password_str: &str,
    ) -> Result<Session, FabricError> {
        let user = self
            .repo
            .find_user_for_auth(username)
            .await?
            .ok_or(FabricError::AuthzDenied)?;

        if user.disabled_at.is_some() || user.auth_source == "development_seed" {
            return Err(FabricError::AuthzDenied);
        }

        let valid = password::verify_password(password_str, &user.password_hash)
            .map_err(|_| FabricError::AuthzDenied)?;
        if !valid {
            return Err(FabricError::AuthzDenied);
        }

        let roles: Vec<Role> = user
            .roles
            .iter()
            .filter_map(|role| Role::from_str(role).ok())
            .collect();

        let token = Uuid::new_v4().to_string();
        let expires = OffsetDateTime::now_utc() + time::Duration::hours(12);

        self.repo
            .create_session(user.user_id, user.tenant_id, &token, expires)
            .await?;

        Ok(Session {
            user_id: user.user_id,
            tenant_id: user.tenant_id,
            username: user.username,
            roles,
            token,
        })
    }

    pub async fn lookup(&self, token: &str) -> Result<Option<Session>, FabricError> {
        match self.repo.find_active_session(token).await? {
            Some(session) => {
                if session.disabled_at.is_some() || session.auth_source == "development_seed" {
                    return Ok(None);
                }

                let roles: Vec<Role> = session
                    .roles
                    .iter()
                    .filter_map(|role| Role::from_str(role).ok())
                    .collect();

                self.repo.touch(&session.token).await?;

                Ok(Some(Session {
                    user_id: session.user_id,
                    tenant_id: session.tenant_id,
                    username: session.username,
                    roles,
                    token: session.token,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn revoke(&self, token: &str) -> Result<(), FabricError> {
        self.repo.revoke(token).await?;
        Ok(())
    }
}
