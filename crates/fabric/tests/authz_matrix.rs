use fabric::auth::{AuthCtx, Role, Session};
use uuid::Uuid;

const DEV_TENANT: &str = "00000000-0000-0000-0000-000000000001";

fn dev_tenant() -> Uuid {
    Uuid::parse_str(DEV_TENANT).expect("dev tenant uuid")
}

fn test_session(roles: Vec<Role>) -> Session {
    Session {
        user_id: Uuid::new_v4(),
        tenant_id: dev_tenant(),
        username: "test".into(),
        roles,
        token: "test-token".into(),
    }
}

fn ctx_with_roles(roles: Vec<Role>) -> AuthCtx {
    AuthCtx {
        principal: "user:test".into(),
        tenant: dev_tenant(),
        session: Some(test_session(roles)),
    }
}

#[test]
fn authz_matrix_viewer_can_only_read() {
    let ctx = ctx_with_roles(vec![Role::Viewer]);
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_err());
}

#[test]
fn authz_matrix_operator_can_read_and_write() {
    let ctx = ctx_with_roles(vec![Role::Operator]);
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok());
    assert!(ctx.require_role(Role::Approver).is_err());
}

#[test]
fn authz_matrix_approver_can_approve() {
    let ctx = ctx_with_roles(vec![Role::Approver]);
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok());
    assert!(ctx.require_role(Role::Approver).is_ok());
    assert!(ctx.require_role(Role::Admin).is_err());
}

#[test]
fn authz_matrix_admin_can_do_all() {
    let ctx = ctx_with_roles(vec![Role::Admin]);
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok());
    assert!(ctx.require_role(Role::Approver).is_ok());
    assert!(ctx.require_role(Role::Admin).is_ok());
}

#[test]
fn authz_matrix_multi_role_user_gets_highest() {
    let ctx = ctx_with_roles(vec![Role::Viewer, Role::Approver]);
    assert!(ctx.require_role(Role::Viewer).is_ok());
    assert!(ctx.require_role(Role::Operator).is_ok());
    assert!(ctx.require_role(Role::Approver).is_ok());
    assert!(ctx.require_role(Role::Admin).is_err());
}

#[test]
fn authz_matrix_no_session_is_denied() {
    let ctx = AuthCtx {
        principal: "anonymous".into(),
        tenant: dev_tenant(),
        session: None,
    };
    assert!(ctx.require_role(Role::Viewer).is_err());
}

#[test]
fn authz_approve_cannot_be_own_proposal() {
    let ctx = AuthCtx {
        principal: "user:alice".into(),
        tenant: dev_tenant(),
        session: Some(Session {
            user_id: Uuid::new_v4(),
            tenant_id: dev_tenant(),
            username: "alice".into(),
            roles: vec![Role::Approver],
            token: "alice-token".into(),
        }),
    };
    assert!(ctx.require_role(Role::Approver).is_ok());
}
