pub mod jwt;
pub mod password;
pub mod session;

pub use session::SessionStore;

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::FabricError;

/// Role-based access control levels.
///
/// Ordering (least to most privileged):
///   Viewer < Operator < Approver < Admin
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Viewer,
    Operator,
    Approver,
    Admin,
}

impl Role {
    /// Returns true when `self` meets or exceeds the `minimum` role.
    ///
    /// ```
    /// # use fabric::auth::Role;
    /// assert!(Role::Admin.require(&Role::Viewer));
    /// assert!(Role::Admin.require(&Role::Admin));
    /// assert!(!Role::Viewer.require(&Role::Operator));
    /// ```
    pub fn require(&self, minimum: &Role) -> bool {
        self.priority() >= minimum.priority()
    }

    fn priority(&self) -> u8 {
        match self {
            Role::Viewer => 0,
            Role::Operator => 1,
            Role::Approver => 2,
            Role::Admin => 3,
        }
    }

    /// Canonical lowercase string representation of this role.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Operator => "operator",
            Role::Approver => "approver",
            Role::Admin => "admin",
        }
    }
}

impl FromStr for Role {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "viewer" => Ok(Role::Viewer),
            "operator" => Ok(Role::Operator),
            "approver" => Ok(Role::Approver),
            "admin" => Ok(Role::Admin),
            other => Err(format!("unknown role '{other}'")),
        }
    }
}

/// Authenticated session attached to a request.
#[derive(Debug, Clone)]
pub struct Session {
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub roles: Vec<Role>,
    pub token: String,
}

/// Authorization context extracted from the incoming request.
#[derive(Debug, Clone)]
pub struct AuthCtx {
    pub principal: String,
    pub tenant: Uuid,
    pub session: Option<Session>,
}

impl AuthCtx {
    /// Check that the authenticated session has at least the given role.
    ///
    /// Returns `AuthzDenied` when the user lacks the required role or
    /// is unauthenticated (no session).
    pub fn require_role(&self, role: Role) -> Result<(), FabricError> {
        match &self.session {
            Some(session) => {
                let ok = session.roles.iter().any(|r| r.require(&role));
                if ok {
                    Ok(())
                } else {
                    Err(FabricError::AuthzDenied)
                }
            }
            None => Err(FabricError::AuthzDenied),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ordering_viewer() {
        assert!(Role::Viewer.require(&Role::Viewer));
        assert!(!Role::Viewer.require(&Role::Operator));
        assert!(!Role::Viewer.require(&Role::Approver));
        assert!(!Role::Viewer.require(&Role::Admin));
    }

    #[test]
    fn role_ordering_admin() {
        assert!(Role::Admin.require(&Role::Viewer));
        assert!(Role::Admin.require(&Role::Operator));
        assert!(Role::Admin.require(&Role::Approver));
        assert!(Role::Admin.require(&Role::Admin));
    }

    #[test]
    fn role_from_str_valid() {
        assert_eq!(Role::from_str("viewer").ok(), Some(Role::Viewer));
        assert_eq!(Role::from_str("operator").ok(), Some(Role::Operator));
        assert_eq!(Role::from_str("approver").ok(), Some(Role::Approver));
        assert_eq!(Role::from_str("admin").ok(), Some(Role::Admin));
    }

    #[test]
    fn role_from_str_invalid() {
        assert!(Role::from_str("superuser").is_err());
        assert!(Role::from_str("").is_err());
    }

    #[test]
    fn role_as_str_roundtrip() {
        for role in &[Role::Viewer, Role::Operator, Role::Approver, Role::Admin] {
            assert_eq!(Role::from_str(role.as_str()).ok(), Some(role.clone()));
        }
    }

    #[test]
    fn authctx_no_session_is_denied() {
        let ctx = AuthCtx {
            principal: "anonymous".into(),
            tenant: Uuid::nil(),
            session: None,
        };
        assert!(ctx.require_role(Role::Viewer).is_err());
        assert!(ctx.require_role(Role::Admin).is_err());
    }

    #[test]
    fn authctx_admin_can_do_all() {
        let ctx = AuthCtx {
            principal: "user:admin".into(),
            tenant: Uuid::nil(),
            session: Some(Session {
                user_id: Uuid::new_v4(),
                tenant_id: Uuid::nil(),
                username: "admin".into(),
                roles: vec![Role::Admin],
                token: "admin-token".into(),
            }),
        };
        assert!(ctx.require_role(Role::Viewer).is_ok());
        assert!(ctx.require_role(Role::Operator).is_ok());
        assert!(ctx.require_role(Role::Approver).is_ok());
        assert!(ctx.require_role(Role::Admin).is_ok());
    }

    #[test]
    fn authctx_multi_role_user_gets_highest() {
        let ctx = AuthCtx {
            principal: "user:op".into(),
            tenant: Uuid::nil(),
            session: Some(Session {
                user_id: Uuid::new_v4(),
                tenant_id: Uuid::nil(),
                username: "op".into(),
                roles: vec![Role::Viewer, Role::Operator],
                token: "op-token".into(),
            }),
        };
        assert!(ctx.require_role(Role::Viewer).is_ok());
        assert!(ctx.require_role(Role::Operator).is_ok());
        assert!(ctx.require_role(Role::Approver).is_err());
    }
}
